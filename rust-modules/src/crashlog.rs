//! The local crash log's image marker: which binary a crash record belongs to. Nothing here
//! leaves the television; `src/crashtrace.c` writes the records and `tools/crash-report.sh` reads
//! them back with `addr2line`.

/// The fallback image base: the lowest `PT_LOAD` vaddr this non-PIE executable links at. Used
/// when [`image_addr`] cannot read the running binary, so the crash log still names the base that
/// `addr2line` needs (an ELF's symbol addresses are relative to it, not to zero).
pub(crate) const IMAGE_ADDR: &str = "0x10000";

/// The image base this binary actually links at, read once from `/proc/self/exe`; [`IMAGE_ADDR`]
/// when that cannot be trusted (no `/proc`, not a little-endian `ET_EXEC` ELF).
pub(crate) fn image_addr() -> &'static str {
    static V: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    V.get_or_init(|| {
        read_head("/proc/self/exe")
            .as_deref()
            .map(image_addr_of)
            .unwrap_or_else(|| IMAGE_ADDR.to_string())
    })
}

/// The pure half of [`image_addr`], so a test grades the expression the app uses.
fn image_addr_of(buf: &[u8]) -> String {
    lowest_load_vaddr(buf)
        .map(|v| format!("0x{v:x}"))
        .unwrap_or_else(|| IMAGE_ADDR.to_string())
}

/// The head of a file: enough for an ELF header and its program headers, not the whole binary.
fn read_head(path: &str) -> Option<Vec<u8>> {
    use std::io::Read;
    let mut buf = Vec::new();
    std::fs::File::open(path)
        .ok()?
        .take(64 * 1024)
        .read_to_end(&mut buf)
        .ok()?;
    Some(buf)
}

/// The GNU build id of the running binary as lowercase hex, empty when it cannot be read. It pairs a
/// crash log with the `nativejelly.debug` of the build that crashed.
pub(crate) fn build_id() -> &'static str {
    static V: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    V.get_or_init(|| {
        read_head("/proc/self/exe")
            .as_deref()
            .and_then(gnu_build_id)
            .unwrap_or_default()
    })
}

/// The mapped span of this executable's loadable ELF segments.
pub(crate) fn image_size() -> u64 {
    static V: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
    *V.get_or_init(|| {
        read_head("/proc/self/exe")
            .as_deref()
            .and_then(load_span)
            .unwrap_or(0)
    })
}

/// Append the running image's identity to the crash log before any signal can arrive (called from
/// `src/main.c`). The signal handler cannot parse ELF or allocate; this line ties every later
/// fallback record to the binary that actually crashed, even if another one is deployed before the
/// log is read.
#[no_mangle]
pub extern "C" fn nj_crash_write_image_marker(fd: std::os::raw::c_int) {
    if fd < 0 {
        return;
    }
    let line = if build_id().is_empty() {
        // Still delimit this process. Otherwise a rare ELF-read failure would let a later crash
        // inherit the preceding process's perfectly valid, but now wrong, build identity.
        "img: unavailable\n".to_string()
    } else {
        format!(
            "img: build_id={} image_addr={} image_size=0x{:x}\n",
            build_id(),
            image_addr(),
            image_size()
        )
    };
    unsafe {
        let _ = libc::write(fd, line.as_ptr().cast(), line.len());
    }
}

/// Walk an ELF's `PT_NOTE` segments for `NT_GNU_BUILD_ID`. Note names and descriptors are each
/// padded to four bytes.
pub(crate) fn gnu_build_id(buf: &[u8]) -> Option<String> {
    const PT_NOTE: u32 = 4;
    const NT_GNU_BUILD_ID: u32 = 3;
    if buf.len() < 64 || &buf[..4] != b"\x7fELF" || buf[5] != 1 {
        return None;
    }
    let is64 = buf[4] == 2;
    let u16at =
        |o: usize| -> Option<u16> { Some(u16::from_le_bytes(buf.get(o..o + 2)?.try_into().ok()?)) };
    let u32at =
        |o: usize| -> Option<u32> { Some(u32::from_le_bytes(buf.get(o..o + 4)?.try_into().ok()?)) };
    let u64at =
        |o: usize| -> Option<u64> { Some(u64::from_le_bytes(buf.get(o..o + 8)?.try_into().ok()?)) };
    let (phoff, phentsize, phnum) = if is64 {
        (
            u64at(32)? as usize,
            u16at(54)? as usize,
            u16at(56)? as usize,
        )
    } else {
        (
            u32at(28)? as usize,
            u16at(42)? as usize,
            u16at(44)? as usize,
        )
    };
    for i in 0..phnum {
        let base = phoff.checked_add(i.checked_mul(phentsize)?)?;
        if u32at(base)? != PT_NOTE {
            continue;
        }
        let (off, size) = if is64 {
            (u64at(base + 8)? as usize, u64at(base + 32)? as usize)
        } else {
            (u32at(base + 4)? as usize, u32at(base + 16)? as usize)
        };
        let mut at = off;
        let end = off.checked_add(size)?;
        while at + 12 <= end && at + 12 <= buf.len() {
            let namesz = u32at(at)? as usize;
            let descsz = u32at(at + 4)? as usize;
            let kind = u32at(at + 8)?;
            let name_at = at + 12;
            let desc_at = name_at.checked_add(namesz.next_multiple_of(4))?;
            let desc_end = desc_at.checked_add(descsz)?;
            if kind == NT_GNU_BUILD_ID && buf.get(name_at..name_at + namesz) == Some(b"GNU\0") {
                let d = buf.get(desc_at..desc_end)?;
                return Some(d.iter().map(|b| format!("{b:02x}")).collect());
            }
            let next = desc_at.checked_add(descsz.next_multiple_of(4))?;
            if next <= at {
                break; // a zero-length note would otherwise spin here forever
            }
            at = next;
        }
    }
    None
}

/// The lowest `PT_LOAD` virtual address in an ELF image; `None` for anything this cannot answer
/// honestly (not an ELF, big-endian, `ET_DYN`, truncated, no `PT_LOAD`).
pub(crate) fn lowest_load_vaddr(buf: &[u8]) -> Option<u64> {
    const PT_LOAD: u32 = 1;
    const ET_EXEC: u16 = 2;
    if buf.len() < 64 || &buf[..4] != b"\x7fELF" {
        return None;
    }
    let is64 = match buf[4] {
        1 => false,
        2 => true,
        _ => return None,
    };
    if buf[5] != 1 {
        return None; // big-endian: every field below is read little-endian
    }
    let u16at =
        |o: usize| -> Option<u16> { Some(u16::from_le_bytes(buf.get(o..o + 2)?.try_into().ok()?)) };
    let u32at =
        |o: usize| -> Option<u32> { Some(u32::from_le_bytes(buf.get(o..o + 4)?.try_into().ok()?)) };
    let u64at =
        |o: usize| -> Option<u64> { Some(u64::from_le_bytes(buf.get(o..o + 8)?.try_into().ok()?)) };

    if u16at(16)? != ET_EXEC {
        return None;
    }
    let (phoff, phentsize, phnum) = if is64 {
        (
            u64at(32)? as usize,
            u16at(54)? as usize,
            u16at(56)? as usize,
        )
    } else {
        (
            u32at(28)? as usize,
            u16at(42)? as usize,
            u16at(44)? as usize,
        )
    };
    let vaddr_at = if is64 { 16 } else { 8 };
    let min_entry = if is64 { 24 } else { 12 };
    if phentsize < min_entry || phnum == 0 {
        return None;
    }
    (0..phnum)
        .filter_map(|i| {
            let base = phoff.checked_add(i.checked_mul(phentsize)?)?;
            if u32at(base)? != PT_LOAD {
                return None;
            }
            if is64 {
                u64at(base + vaddr_at)
            } else {
                u32at(base + vaddr_at).map(u64::from)
            }
        })
        .min()
}

/// Size of the address range covered by all `PT_LOAD` segments in one `ET_EXEC` ELF.
fn load_span(buf: &[u8]) -> Option<u64> {
    const PT_LOAD: u32 = 1;
    const ET_EXEC: u16 = 2;
    if buf.len() < 64 || &buf[..4] != b"\x7fELF" || buf[5] != 1 {
        return None;
    }
    let is64 = match buf[4] {
        1 => false,
        2 => true,
        _ => return None,
    };
    let u16at =
        |o: usize| -> Option<u16> { Some(u16::from_le_bytes(buf.get(o..o + 2)?.try_into().ok()?)) };
    let u32at =
        |o: usize| -> Option<u32> { Some(u32::from_le_bytes(buf.get(o..o + 4)?.try_into().ok()?)) };
    let u64at =
        |o: usize| -> Option<u64> { Some(u64::from_le_bytes(buf.get(o..o + 8)?.try_into().ok()?)) };
    if u16at(16)? != ET_EXEC {
        return None;
    }
    let (phoff, phentsize, phnum) = if is64 {
        (
            u64at(32)? as usize,
            u16at(54)? as usize,
            u16at(56)? as usize,
        )
    } else {
        (
            u32at(28)? as usize,
            u16at(42)? as usize,
            u16at(44)? as usize,
        )
    };
    let minimum = if is64 { 48 } else { 24 };
    if phentsize < minimum || phnum == 0 {
        return None;
    }
    let mut low = u64::MAX;
    let mut high = 0u64;
    for i in 0..phnum {
        let base = phoff.checked_add(i.checked_mul(phentsize)?)?;
        if u32at(base)? != PT_LOAD {
            continue;
        }
        let (vaddr, memsz) = if is64 {
            (u64at(base + 16)?, u64at(base + 40)?)
        } else {
            (u64::from(u32at(base + 8)?), u64::from(u32at(base + 20)?))
        };
        low = low.min(vaddr);
        high = high.max(vaddr.checked_add(memsz)?);
    }
    (low != u64::MAX && high > low).then_some(high - low)
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn the_image_base_is_the_load_vaddr_not_zero() {
        assert_eq!(IMAGE_ADDR, "0x10000");
        assert_ne!(
            IMAGE_ADDR, "0x0",
            "a zero image base silently yields missing_symbol"
        );
    }

    fn elf32(phdrs: &[[u32; 8]], notes: &[u8], note_off: u32) -> Vec<u8> {
        const EHDR: usize = 52;
        const PHENT: usize = 32;
        let mut v = vec![0u8; EHDR];
        v[..4].copy_from_slice(b"\x7fELF");
        v[4] = 1; // ELFCLASS32
        v[5] = 1; // ELFDATA2LSB
        v[6] = 1;
        v[16..18].copy_from_slice(&2u16.to_le_bytes()); // ET_EXEC
        v[18..20].copy_from_slice(&40u16.to_le_bytes()); // EM_ARM
        v[28..32].copy_from_slice(&(EHDR as u32).to_le_bytes()); // e_phoff
        v[40..42].copy_from_slice(&(EHDR as u16).to_le_bytes()); // e_ehsize
        v[42..44].copy_from_slice(&(PHENT as u16).to_le_bytes()); // e_phentsize
        v[44..46].copy_from_slice(&(phdrs.len() as u16).to_le_bytes()); // e_phnum
        for ph in phdrs {
            for w in ph {
                v.extend_from_slice(&w.to_le_bytes());
            }
        }
        v.resize(note_off.max(v.len() as u32) as usize, 0);
        v.extend_from_slice(notes);
        v.resize(v.len().max(64), 0); // the parser refuses anything shorter than an ELF64 header
        v
    }

    fn note(name: &[u8], kind: u32, desc: &[u8]) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&(name.len() as u32).to_le_bytes());
        v.extend_from_slice(&(desc.len() as u32).to_le_bytes());
        v.extend_from_slice(&kind.to_le_bytes());
        v.extend_from_slice(name);
        v.resize(v.len().next_multiple_of(4), 0);
        v.extend_from_slice(desc);
        v.resize(v.len().next_multiple_of(4), 0);
        v
    }

    #[test]
    fn the_image_base_is_the_lowest_load_segment() {
        //          p_type      offset  vaddr     paddr  filesz  memsz  flags  align
        let data = [1u32, 0x8000, 0x30000, 0, 0x100, 0x100, 6, 0x1000];
        let text = [1u32, 0x0000, 0x10000, 0, 0x8000, 0x8000, 5, 0x1000];
        let phdr = [6u32, 0x34, 0x10034, 0, 0x40, 0x40, 4, 4];
        // Deliberately NOT in address order, and with a non-LOAD segment first.
        let b = elf32(&[phdr, data, text], &[], 0);
        assert_eq!(lowest_load_vaddr(&b), Some(0x10000));
        assert_eq!(
            load_span(&b),
            Some(0x20100),
            "size reaches the end of the highest LOAD"
        );
        assert_eq!(
            image_addr_of(&b),
            "0x10000",
            "and it agrees with the measured constant"
        );
    }

    #[test]
    fn anything_unanswerable_is_refused_rather_than_guessed() {
        let text = [1u32, 0, 0x10000, 0, 0x8000, 0x8000, 5, 0x1000];
        let mut dynamic = elf32(&[text], &[], 0);
        dynamic[16..18].copy_from_slice(&3u16.to_le_bytes()); // ET_DYN: vaddr is not the load address
        assert_eq!(lowest_load_vaddr(&dynamic), None);

        let mut be = elf32(&[text], &[], 0);
        be[5] = 2; // big-endian: every field below is read little-endian
        assert_eq!(lowest_load_vaddr(&be), None);

        assert_eq!(
            lowest_load_vaddr(b"not an elf at all, not remotely, no"),
            None
        );
        assert_eq!(lowest_load_vaddr(&elf32(&[], &[], 0)), None, "no PT_LOAD");
        // A header whose `e_phoff` points past the end of what was read. Not a corruption case so
        // much as the ordinary bound on `read_head`: this parser sees the first 64 KiB, and an
        // unreadable table must be "I do not know" rather than an entry read out of whatever bytes
        // happened to be at that offset.
        let mut far = elf32(&[text], &[], 0);
        far[28..32].copy_from_slice(&0x10_0000u32.to_le_bytes());
        assert_eq!(lowest_load_vaddr(&far), None);
    }

    #[test]
    fn the_build_id_note_is_found_behind_another_note() {
        let id: Vec<u8> = (0u8..20).collect();
        let mut notes = note(b"Linux\0", 0x100, &[1, 2, 3]); // namesz 6 -> two bytes of padding
        notes.extend_from_slice(&note(b"GNU\0", 3, &id));
        let off = 52 + 32 * 2; // after two program headers
        let seg = [4u32, off, 0, 0, notes.len() as u32, 0, 4, 4]; // PT_NOTE
        let text = [1u32, 0, 0x10000, 0, 0x8000, 0x8000, 5, 0x1000];
        let b = elf32(&[seg, text], &notes, off);
        assert_eq!(
            gnu_build_id(&b).as_deref(),
            Some("000102030405060708090a0b0c0d0e0f10111213")
        );
    }
}
