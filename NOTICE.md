# Notices and attribution

Vitals is an independent project. Its visual language and the first version of its Direct2D render kit were
adapted from two MIT-licensed projects by **joxan2137**:

- [Glint](https://github.com/joxan2137/glint) — the reference for the calm, layered surface treatment, typography
  and spring motion.
- [Tuck](https://github.com/joxan2137/tuck) — the source of the `tuck-ui` crate that became `vitals-ui`, including
  the window/compositor loop, painter, widgets, icons, theme and offscreen renderer.

Both upstream projects carry the following license:

```text
MIT License

Copyright (c) 2026 joxan2137

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

Vitals itself is distributed under the license in [`LICENSE`](LICENSE). The process engine, Windows integration,
views, application shell and documentation are original work.

## Direct dependencies

| Crate | License | Use |
|---|---|---|
| [`anyhow`](https://crates.io/crates/anyhow) | MIT OR Apache-2.0 | Error context |
| [`embed-resource`](https://crates.io/crates/embed-resource) | MIT | Windows icon and manifest embedding |
| [`log`](https://crates.io/crates/log) | MIT OR Apache-2.0 | Diagnostics |
| [`png`](https://crates.io/crates/png) | MIT OR Apache-2.0 | PNG preview export |
| [`serde`](https://crates.io/crates/serde) | MIT OR Apache-2.0 | Settings serialization |
| [`serde_json`](https://crates.io/crates/serde_json) | MIT OR Apache-2.0 | Settings storage |
| [`windows`](https://crates.io/crates/windows) | MIT OR Apache-2.0 | Win32, Direct2D, DirectWrite, DirectComposition |

The icon is original artwork rendered by [`tools/make-icon.ps1`](tools/make-icon.ps1). The Lucide icon geometry in
`vitals-ui` is derived from the Lucide icon set, which is distributed under the ISC license.
