# Web Search

Type `s ` in Waymenu to list your web search shortcuts. Select one and press
Tab to complete its shortcut, or type it directly. Press Enter on a search
result or bookmark to open it in your browser.
Tab on a bookmark fills in its name and keeps the shortcut. For example,
`s b whatsapp` becomes `s b WhatsApp Web`.

To build and install the plugin from source:

```sh
cargo build --release --locked --manifest-path plugins/web-search/Cargo.toml
install -Dm755 plugins/web-search/target/release/libweb_search.so \
  "${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/plugins/libweb_search.so"
```

To migrate from the old `g ` and `yt ` plugins, move any installed
`libgoogle.so` and `libyoutube.so` out of the Waymenu plugin directory.

## Add a shortcut

Create the configuration directory and add one `.toml` file per shortcut:

```sh
mkdir -p "${XDG_CONFIG_HOME:-$HOME/.config}/waymenu/web-search"
```

The filename without `.toml` is the shortcut after `s `. Use lowercase ASCII
letters, digits, hyphens, or underscores. TOML keeps bookmark names and URLs
separate, so names can contain spaces. Reopen Waymenu after editing the files.

### `g.toml` for Google

```toml
name = "Google"
type = "search"
url = "https://www.google.com/search?q={query}"
```

Type `s g hyprland window manager`. Waymenu URL-encodes the query and substitutes
it for `{query}`. A search template takes an HTTP or HTTPS URL with one
`{query}` in the query string.

### `yt.toml` for YouTube

```toml
name = "YouTube"
type = "search"
url = "https://www.youtube.com/results?search_query={query}"
```

Type `s yt hyprland window manager` and press Enter on the search result.

### `b.toml` for bookmarks

```toml
name = "Bookmarks"
type = "bookmarks"

[[bookmarks]]
name = "WhatsApp Web"
url = "https://web.whatsapp.com/"

[[bookmarks]]
name = "Next.js Installation"
url = "https://nextjs.org/docs/app/getting-started/installation"
```

Type `s b` to list the bookmarks or `s b whatsapp` to filter by name or URL.
Use the same format in `next.toml` for a separate list under `s next`.
