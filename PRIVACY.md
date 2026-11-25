# JellyRust Privacy Policy

The app shows a shorter version of this policy under Settings, Legal notices, Privacy policy.

## Who is responsible

macluckylab is responsible only for the data JellyRust stores locally on the television. Contact:
<https://github.com/mac-lucky/jellyrust/issues>.

## Your Jellyfin server

JellyRust is an independent client for Jellyfin. It has no account, online service or reporting of
its own and sends nothing anywhere except to your Jellyfin server: signing in, browsing and playing
media, watch progress and every other server feature go directly to the Jellyfin server whose
address you enter, and are handled by that server and whoever operates it. JellyRust's developer
does not receive any of it.

JellyRust contains no crash-reporting or analytics code. The optional crash reports and product
analytics that Native Jelly offers were removed from this fork, together with the question that
asks for them.

When you enter an address without `http://` or `https://`, JellyRust tries plain http first for an
address on your home network and https first for a domain name, and falls back to the other if the
first does not answer. Over http, your password, access token and everything JellyRust exchanges
with the server travel over the network without encryption. Enter the full `https://` address for
a server you reach over the internet.

JellyRust sends nothing that signs you in over plain http until you allow it for that server. It
asks once, naming the server and saying whether its address is on your home network or the
internet. The answer covers that server at that address only. Each time JellyRust connects, it
first checks, without sending anything that signs you in, that the same server answers there. The
answers are stored on this television without any user name or token. They are kept when you sign
out; Settings > Unencrypted connections lists them and turns each one off, and Delete all local
data removes them.

## On this television

JellyRust stores the address, name, identifier and version of the Jellyfin server you signed in to,
and for each person signed in on this television (up to twelve, listed on the Who's watching?
screen) their user name and user identifier there, the access token that server issued to this
television and the device identifier it was issued under, in one file created with mode 0600. It
never stores your password.

It also keeps downloaded artwork (posters, backdrops and cast images, cached as bounded files for
reuse across restarts), each person's Home library choices, recent searches and subtitle timing
adjustments, kept apart from everyone else's, your playback quality, Direct Play and app language
preferences, which the whole television shares, and local technical logs: a small rotating event
log and a bounded storage status snapshot. It keeps no bookmark of its own for where you stopped
watching: playback position is held by your Jellyfin server.

Development builds have an input and frame recorder used to reproduce bugs. A release build, the
one you install, has it compiled out, and its recordings never leave the device they were made on.

## Retention

Signing out removes that one person's Jellyfin sign-in from this television (their user name and
identifier there, and the access token) and asks the server to revoke the token; everyone else
signed in stays. When the last person signs out, the address, name and identifier of the server
signed in to last are kept, with no user or token, so the sign-in screen can offer it under Recent;
signing in to another server replaces it.

**Signing out does not currently remove anything else JellyRust keeps**, including that server and
the signed-out person's library choices, searches and subtitle adjustments: use Delete all local
data to remove those. The event log rotates continuously and the storage snapshot is replaced when
its bounded status changes. The app language preference belongs to this television and is kept
when you sign out. Delete all local data signs everyone out, asks the server to revoke every token,
and removes all of it, including the server under Recent and the language preference.

## Your choices

There is no reporting to turn on or off. Delete all local data in Settings removes everything
JellyRust stored on this television. What your Jellyfin server keeps, such as watch progress and
favourites, is managed on that server.

## Where data is processed

Neither JellyRust's developer nor any third party processes anything JellyRust does. A Jellyfin
server you connect to may be located anywhere and is operated by whoever runs it, not by
JellyRust's developer.

## Uninstalling

Removing JellyRust removes the application, but webOS gives an application no way to run code as
it is removed, so anything kept outside the application's own directory can survive. Your sign-in
is deliberately kept there so that reinstalling does not sign you out, and so is your app language
preference. Use Delete all local data **before** uninstalling if you want nothing of JellyRust left
on this television.

## Contact and non-affiliation

Privacy questions: <https://github.com/mac-lucky/jellyrust/issues>. Issues there are public, so do
not post anything you would not want others to read. Report security vulnerabilities privately at
<https://github.com/mac-lucky/jellyrust/security/advisories/new>.

JellyRust is an independent, unofficial application. It is not produced by, endorsed by, or
affiliated with the Jellyfin project, LG Electronics Inc., Native Jelly, PlxNative or Litefin.
