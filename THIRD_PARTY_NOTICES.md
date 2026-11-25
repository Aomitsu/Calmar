# Third-party notices

Calmar itself is licensed under the GNU Affero General Public License v3.0 or
later (`AGPL-3.0-or-later`). See [LICENSE.md](LICENSE.md).

Calmar depends on the following third-party components. Their licenses are
listed here as required. Unless stated otherwise, the notices below cover the
version in use at build time.

## Rust dependencies

| Component | Version | License |
|---|---|---|
| iced | 0.14 | MIT |
| iced_fonts | 0.3 | MIT |
| rfd | 0.18 | MIT |
| serde | 1.0 | MIT OR Apache-2.0 |
| toml | 1.2 | MIT OR Apache-2.0 |
| git2 | 0.21 | MIT OR Apache-2.0 |
| libgit2-sys | 0.18 | MIT OR Apache-2.0 |
| thiserror | 2.0 | MIT OR Apache-2.0 |
| dunce | 1.0 | CC0-1.0 OR MIT-0 OR Apache-2.0 |
| jiff | 1.0 | Unlicense OR MIT |
| pulldown-cmark | 0.12 | MIT |

Transitive dependencies are covered by their own licenses, which Cargo
includes in each crate it builds. The full dependency graph and its licenses
can be regenerated with `cargo metadata`.

## Bundled libgit2

`git2` links against libgit2, bundled and compiled by `libgit2-sys`.

> libgit2 is Copyright (C) the libgit2 contributors. It is distributed under
> the GNU General Public License version 2, with a linking exception: the
> authors give unlimited permission to link the compiled version of this
> library into combinations with other programs, and to distribute those
> combinations without any restriction coming from this file.

## Lucide icons

The icon font embedded by `iced_fonts` is [Lucide](https://lucide.dev).

> ISC License
>
> Copyright (c) 2026 Lucide Icons and Contributors
>
> Permission to use, copy, modify, and/or distribute this software for any
> purpose with or without fee is hereby granted, provided that the above
> copyright notice and this permission notice appear in all copies.
>
> THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES
> WITH REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF
> MERCHANTABILITY AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR
> ANY SPECIAL, DIRECT, INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES
> WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS, WHETHER IN AN
> ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION, ARISING OUT OF
> OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS SOFTWARE.

Some Lucide icons are derived from the Feather project and are MIT licensed:

> The MIT License (MIT) (for the Feather-derived icons)
>
> Copyright (c) 2013-present Cole Bemis
>
> Permission is hereby granted, free of charge, to any person obtaining a copy
> of this software and associated documentation files (the "Software"), to deal
> in the Software without restriction, including without limitation the rights
> to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
> copies of the Software, and to permit persons to whom the Software is
> furnished to do so, subject to the following conditions:
>
> The above copyright notice and this permission notice shall be included in
> all copies or substantial portions of the Software.
