//! Library paint consumes the same placement queries as keyboard and pointer navigation.
use super::*;
use crate::screens::registry::tile_facts;
use crate::ui::card_row;
use crate::ui::screen::{Activate, Hover, Stop};
use crate::ui::theme;
use crate::ui::widgets::Art;
use crate::ui::{Env, View, on_axis};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Layer { Grid, Document, Rail }

/// Paint and stop registration share one back-to-front order.
fn layers(mut visit: impl FnMut(Layer)) {
    for layer in [Layer::Grid, Layer::Document, Layer::Rail] { visit(layer); }
}

fn shelf_on_screen(origin: f32, pitch: f32) -> bool {
    on_axis(origin - crate::ui::consts::TITLE_DY, pitch, SCR_H, 0.0)
}

/// Reject a whole control band before creating labels or measuring individual pills. The
/// full-width envelope preserves travelling capsules; shared cast tokens cover their shadows.
/// It is deliberately wider than the controls, so no text metrics are needed to reject it.
fn document_band_visible(p: crate::ui::Painter, y: f32, height: f32, pop: f32) -> bool {
    let pad = theme::CONTROL_CAST_FOCUS.iter()
        .map(|(dy, blur, _)| dy.abs() + blur).fold(0.0f32, f32::max) + 1.0;
    let mut bounds = Rect::new(0.0, y, SCR_W, height)
        .scaled(pop.max(1.0) * p.scale().max(1.0)).inset(-pad);
    bounds.x += p.dx();
    bounds.y += p.dy();
    let visible = bounds.intersect(Rect::FULL);
    visible.w > 0.0 && visible.h > 0.0
        && (crate::ui::frame::backdrop::discovering()
            || !nj_gfx::gfx::culled(bounds.x, bounds.y, bounds.w, bounds.h))
}

#[cfg(test)]
mod layer_tests {
    #[test]
    fn document_band_culling_preserves_partial_labels_focus_and_shadow_edges() {
        let _guard = nj_base::testlock::serial();
        let p = crate::ui::Painter::root();
        let height = crate::ui::widgets::StatusOverlay::CTRL_H;
        let pad = crate::ui::theme::CONTROL_CAST_FOCUS.iter()
            .map(|(dy, blur, _)| dy.abs() + blur).fold(0.0f32, f32::max) + 1.0;
        assert!(super::document_band_visible(p, -height + 1.0, height, 1.0));
        assert!(super::document_band_visible(p, -height - pad + 1.0, height, 1.0),
            "a focus shadow can remain on the panel after the control itself left");
        let above = -height - pad - 1.0;
        assert!(!super::document_band_visible(p, above, height, 1.0));
        assert!(super::document_band_visible(p, above, height, crate::ui::widgets::CTRL_FOCUS_SCALE),
            "the focused capsule's growth belongs in the paint envelope");
        assert!(super::document_band_visible(p.translate(0.0, 300.0), above, height, 1.0));
        assert!(!super::document_band_visible(p, super::SCR_H + pad + 1.0, height, 1.0));
        assert!(!super::document_band_visible(p, -2000.0, super::layout::GRID_HEAD_H, 1.0));
    }

    #[test]
    fn document_band_culling_keeps_visible_controls_during_backdrop_discovery() {
        let _guard = nj_base::testlock::serial();
        let _discovery = crate::ui::frame::backdrop::discover(
            std::rc::Rc::new(std::cell::RefCell::new(Default::default())));
        assert!(nj_gfx::gfx::culled(0.0, 0.0, 100.0, 100.0),
            "discovery suppresses GL draws without suppressing paint declarations");
        assert!(super::document_band_visible(crate::ui::Painter::root(),
            super::CONTENT_TOP, super::layout::GRID_HEAD_H, 1.0));
    }

    #[test]
    fn shelf_culling_keeps_a_shelf_poking_in_at_the_top_edge() {
        use crate::ui::consts::{CARD_H, ROW_PITCH, SCR_H, TITLE_DY};
        let y = 8.0 - CARD_H;
        assert!(y + CARD_H > 0.0);
        assert!(super::shelf_on_screen(y, ROW_PITCH));
        assert!(!super::shelf_on_screen(TITLE_DY - ROW_PITCH - 1.0, ROW_PITCH));
        assert!(!super::shelf_on_screen(SCR_H + TITLE_DY + 1.0, ROW_PITCH));
    }

    #[test]
    fn library_paint_and_stop_order_keeps_controls_above_grid_and_rail_above_document() {
        use super::{layers, Layer};
        let mut seen = Vec::new();
        layers(|layer| seen.push(layer));
        assert_eq!(seen, [Layer::Grid, Layer::Document, Layer::Rail]);
        for layer in [Layer::Grid, Layer::Document, Layer::Rail] {
            assert_eq!(seen.iter().filter(|&&found| found == layer).count(), 1);
        }
    }
}

impl LibraryScreen {
    pub(super) fn draw_page<H: LibraryLike>(&mut self, f: &mut DrawFrame<'_, '_, H>) {
        // **The Library's own phase names**, in the `hm.*`/`st.*` family — `/tmp/nativejelly-cpuprof`
        // times each on the render thread (inclusive wall, no `glFinish`, every phase at once) and
        // `--graphics-profile --profile-phase <name>` samples one. They exist because the owned
        // screen shipped with NONE: the 2026-09-09 `fps:library-switch` profile read `frame.ui`
        // 24.5 ms and `main.ui` 22.4 with nothing named beneath, so the whole page was one opaque
        // block and the only attribution left was the draw-MASK census, which can subtract a class
        // but cannot say which part of the screen spent it.
        //
        // The page is Litefin's flat black (`docs/litefin-look.md` 2.1): the clear is its whole
        // ground, with no ambient wash. `lb.document` brackets the head, the shelf band and the
        // status read-out with `lb.shelves` inside it for the card rows alone; `lb.grid` is the
        // poster wall's windowed rows and `lb.rail` the alphabet bar.
        crate::ui::profile::phase("lb.clear", || {
            nj_gfx::gfx::frame_clear(theme::CLEAR_RGB.0, theme::CLEAR_RGB.1, theme::CLEAR_RGB.2);
        });
        let layout = self.pair.layout();
        let alpha = self.page_fade.alpha() * self.grid_fade.alpha();
        layers(|layer| match layer {
            Layer::Grid => crate::ui::profile::phase("lb.grid", || {
                draw_faded_part_at(&mut self.pair.detail, f, layout.detail, alpha)
            }),
            Layer::Document => crate::ui::profile::phase("lb.document", || self.draw_document(f)),
            Layer::Rail => crate::ui::profile::phase("lb.rail", || {
                // Decoration derives from the CURRENT engine key or, off the grid, the first title
                // on screen; no remembered cursor is copied.
                let index = f.focus.current.filter(|key| key.entry == self.entry)
                    .and_then(|key| self.pair.detail.index_of(key.elem))
                    .or_else(|| Some(self.layout.visible_rows(self.scroll.pos).0 * self.layout.cols()));
                let parent = f.page_alpha;
                f.page_alpha = parent * alpha;
                self.pair.master.draw_with_current(f, layout.master, index);
                f.page_alpha = parent;
            }),
        });
        self.plaintext_alert.draw(f, self.entry);
    }

    fn draw_document<H: LibraryLike>(&self, f: &mut DrawFrame<'_, '_, H>) {
        let p = f.painter.alpha(f.page_alpha * self.page_fade.alpha());
        let env = Env::inert();
        self.draw_head(f);
        crate::ui::profile::phase("lb.shelves", || {
            for (index, row) in self.shelves.iter().enumerate() {
                let Some(shelf) = self.hub(index, f.cx) else { continue };
                let origin = self.layout.shelf_y(index, self.scroll.pos);
                if !shelf_on_screen(origin, self.layout.shelf_pitch(index)) { continue; }
                let heading_y = origin - crate::ui::consts::TITLE_DY - row.motion.lift();
                if let Some(face) = self.genre_face(index, f.cx, At::Drawn) {
                    let focused = f.focus.current.is_some_and(|key| key.entry == self.entry
                        && Some(key.elem) == row.heading_elem());
                    self.draw_genre_heading(p, &shelf.title, face, heading_y, focused);
                } else if let Some(heading) = self.heading_widget(index, f.cx) {
                    let focused = f.focus.current.is_some_and(|key| key.entry == self.entry
                        && Some(key.elem) == row.heading_elem());
                    heading.draw(p, LEFT, heading_y, f32::from(focused), &heading.measure(f.measure), f.measure);
                } else {
                    card_row::draw_heading(p, &shelf.title, "", LEFT,
                        heading_y, layout::RIGHT - LEFT, f.measure);
                }
                let focused = f.focus.current.and_then(|key| row.elems.iter().position(|elem| *elem == key.elem));
                for col in 0..row.elems.len() {
                    if focused == Some(col) { continue; }
                    self.draw_shelf_tile(index, col, false, f);
                }
                if let Some(col) = focused {
                    self.draw_shelf_tile(index, col, true, f);
                }
            }
        });
        self.draw_grid_header(f);
        if self.genres == GenreView::Rows {
            // The Genres tab reads its own rows: they are loading, or the server lists none.
            if self.shelves.is_empty() && shelves_settled(H::section_hubs(f.cx)) {
                crate::ui::widgets::StatusOverlay::new(self.status_frame(),
                    nj_platform::i18n::msg::browse_litefin_genres_empty_c(), crate::ui::widgets::StatusKind::Empty)
                    .draw_measured(&env, p, f.cx.measure);
            } else if self.shelves.is_empty() {
                crate::ui::widgets::Spinner::new(SCR_W * 0.5, SCR_H * 0.52, 26.0)
                    .phase(f.cx.tick.ms).draw(&env, f.painter.alpha(f.page_alpha));
            }
        } else if self.readout == Readout::Loading {
            // Preserve the Library's standalone loading spinner, outside either content fade.
            crate::ui::widgets::Spinner::new(SCR_W * 0.5, SCR_H * 0.52, 26.0)
                .phase(f.cx.tick.ms).draw(&env, f.painter.alpha(f.page_alpha));
        } else if self.readout != Readout::Grid {
            let (text, reason) = self.status_text(f.cx);
            let status = self.status_overlay(f.cx, &text, reason.as_deref());
            let alpha = if self.readout == Readout::Empty { self.page_fade.alpha() * self.grid_fade.alpha() } else { 1.0 };
            status.draw_measured(&env, f.painter.alpha(f.page_alpha * alpha), f.cx.measure);
        }
        if !self.plaintext_alert.visible() {
            // the question owns the pointer while it is up; nothing under it is a target
            self.record_document_stops(f);
        }
    }

    /// A genre row's heading: the white .15 face with its 3px accent border under focus, then
    /// the genre's name with its cap top at `cap_y`.
    fn draw_genre_heading(&self, p: crate::ui::Painter, title: &str, face: Rect, cap_y: f32, focused: bool) {
        if focused {
            p.rect(face, theme::radius::BASE, theme::browse_lf::GENRE_HEADER_FOCUS, theme::browse_lf::GENRE_HEADER_FOCUS, 0.0);
            p.rring(face, theme::radius::BASE, theme::FOCUS_RING_W, theme::browse_lf::GENRE_HEADER_RING);
        }
        let title = std::ffi::CString::new(title).unwrap_or_default();
        crate::ui::label::Label::new(title.as_ptr(), theme::size::lf::S30, theme::TEXT_PRIMARY).bold()
            .v(crate::ui::label::VAlign::CapTop).draw(p, Rect::new(LEFT, cap_y, face.w, 0.0));
    }

    /// The page title (32px bold at x=150) and, outside one genre, the centred tab pills.
    fn draw_head<H: LibraryLike>(&self, f: &DrawFrame<'_, '_, H>) {
        let p = f.painter.alpha(f.page_alpha * self.page_fade.alpha());
        let y = self.layout.title_y(self.scroll.pos);
        if !document_band_visible(p, y, layout::TOP_H, crate::ui::widgets::CTRL_FOCUS_SCALE) { return; }
        let title = std::ffi::CString::new(self.title(f.cx)).unwrap_or_default();
        crate::ui::label::Label::new(title.as_ptr(), theme::size::lf::S32, theme::TEXT_PRIMARY).bold()
            .draw(p, Rect::new(LEFT, y, layout::RIGHT - LEFT, layout::TITLE_H));
        if !self.layout.tabs() { return; }
        for (i, tab) in self.tabs().into_iter().enumerate() {
            let elem = TAB_BASE + i as u32;
            let Some((rect, scale)) = self.tab_rect(elem, f.cx, At::Drawn) else { continue };
            let focused = f.focus.current.is_some_and(|key| key.entry == self.entry && key.elem == elem);
            self.tab_pill(tab, f.cx).draw(p, rect, scale, focused, f.measure);
        }
    }

    /// The controls row over the grid: the count, then Shuffle, Sort, Filter and View Mode.
    fn draw_grid_header<H: LibraryLike>(&self, f: &DrawFrame<'_, '_, H>) {
        let p = f.painter.alpha(f.page_alpha * self.page_fade.alpha());
        let y = self.layout.controls_y(self.scroll.pos);
        if !self.layout.grid_head || !document_band_visible(p, y, layout::CONTROL_H, crate::ui::widgets::CTRL_FOCUS_SCALE) { return; }
        if H::listing(f.cx).total() > 0 {
            let count = self.count_text(f.cx);
            crate::ui::label::Label::new(count.as_ptr(), Self::count_rung(), theme::TEXT_SECONDARY)
                .draw(p, Rect::new(self.count_x(f.cx), y, self.count_w(f.cx), layout::CONTROL_H));
        }
        for &elem in self.toolbar_elems() {
            let Some((rect, scale)) = self.control_rect(elem, f.cx, At::Drawn) else { continue };
            let focused = f.focus.current.is_some_and(|key| key.entry == self.entry && key.elem == elem);
            self.control_pill(elem, f.cx).draw(p, rect, scale, focused, f.measure);
        }
    }

    fn record_document_stops<H: LibraryLike>(&self, f: &mut DrawFrame<'_, '_, H>) {
        if !f.records_stops() { return; }
        for (index, row) in self.shelves.iter().enumerate() {
            let origin = self.layout.shelf_y(index, self.scroll.pos);
            if !shelf_on_screen(origin, self.layout.shelf_pitch(index)) { continue; }
            let focused = f.focus.current.filter(|key| key.entry == self.entry).map(|key| key.elem);
            for &elem in row.elems.iter().filter(|elem| Some(**elem) != focused) { self.stop(elem, f); }
            if let Some(elem) = focused.filter(|elem| row.elems.contains(elem)) { self.stop(elem, f); }
            // After the row's cards, so the heading wins where a popped card's glow overlaps it.
            if let (Some(elem), true) = (row.heading_elem(), self.genre_face(index, f.cx, At::Drawn).is_some()) {
                self.stop(elem, f);
            } else if let (Some(elem), Some(heading), Some(rect)) = (row.heading_elem(),
                self.heading_widget(index, f.cx), self.heading_rect(index, f.cx, At::Drawn)) {
                heading.stop(f, rect, self.key(elem));
            }
        }
        if self.layout.tabs() { for elem in self.tab_elems() { self.stop(elem, f); } }
        if self.layout.grid_head { for &elem in self.toolbar_elems() { self.stop(elem, f); } }
        if self.readout == Readout::Failed { self.stop(RETRY, f); }
    }

    /// The production stop producers without rasterization, for the no-SDL dispatcher fixture.
    #[cfg(test)]
    pub(crate) fn record_stops<H: LibraryLike>(&self, f: &mut DrawFrame<'_, '_, H>) {
        layers(|layer| match layer {
            Layer::Grid => self.pair.detail.record_stops(f),
            Layer::Document => self.record_document_stops(f),
            Layer::Rail => self.pair.master.record_stops(f),
        });
    }

    fn stop<H: LibraryLike>(&self, elem: u32, f: &mut DrawFrame<'_, '_, H>) {
        let Some(placed) = <Self as Focusable<H>>::place(self, &elem, f.cx, At::Drawn) else { return };
        f.stop(f.painter, Stop {
            key: self.key(elem), rect: placed.rect, rest_rect: placed.rest_rect, clip: placed.clip,
            hover: Hover::Focus, activate: Activate::Press,
        });
    }

    fn draw_shelf_tile<H: LibraryLike>(&self, row: usize, col: usize, focused: bool, f: &DrawFrame<'_, '_, H>) {
        let Some(shelf) = self.hub(row, f.cx) else { return };
        let Some(item) = shelf.items.get(col) else { return };
        let model = &self.shelves[row];
        let style = row_style(model);
        let mut rect = self.shelf_rect(row, col);
        let scale = model.motion.scale(col) * if focused && f.press.scale > 0.0 { f.press.scale } else { 1.0 };
        if focused && f.press.scale > 0.0 { rect = rect.scaled(f.press.scale); }
        if !on_axis(rect.x, rect.w, SCR_W, 32.0) { return; }
        let p = f.painter.alpha(f.page_alpha * self.page_fade.alpha());
        let art = if shelf.landscape { Art::Still(Some(tile_facts::of(item))) } else { Art::Poster(Some(tile_facts::of(item))) };
        // The card names itself from its row (`card_row::tile_label`); the label lends only where
        // a focused card's row will rest.
        let label = if focused {
            card_row::TileLabel::default().settling(model.motion.settle_lag(model.elems.len(), col, style))
        } else { card_row::TileLabel::default() };
        card_row::draw_cell(p, art, rect, scale, style, item.resume_frac(), &label, focused, f.measure);
    }

    pub(crate) fn redraw_focused<H: LibraryLike>(&self, f: &mut DrawFrame<'_, '_, H>, focus: Option<FocusKey<u32>>) {
        let Some(key) = focus.filter(|key| key.entry == self.entry) else { return };
        let Some(placed) = <Self as Focusable<H>>::place(self, &key.elem, f.cx, At::Drawn) else { return };
        let _clip = f.clip(f.painter, placed.clip);
        if self.pair.detail.index_of(key.elem).is_some() {
            let parent = f.page_alpha;
            f.page_alpha *= self.page_fade.alpha() * self.grid_fade.alpha();
            self.pair.detail.draw_focused(f, Some(key));
            f.page_alpha = parent;
            return;
        }
        if let Some((row, col)) = self.shelves.iter().enumerate().find_map(|(row, shelf)|
            shelf.elems.iter().position(|elem| *elem == key.elem).map(|col| (row, col))) {
            self.draw_shelf_tile(row, col, true, f);
        }
    }
}

fn draw_faded_part_at<H: LibraryLike>(part: &mut impl Part<H>, f: &mut DrawFrame<'_, '_, H>, rect: Rect, alpha: f32) {
    let parent = f.page_alpha;
    f.page_alpha = parent * alpha;
    part.draw(f, rect);
    f.page_alpha = parent;
}
