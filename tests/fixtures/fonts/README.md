# Embedded-font test fixture

`demo.ttf` is the unmodified 400-byte, two-glyph fixture from
[ttf-parser 0.25.1](https://github.com/harfbuzz/ttf-parser/tree/v0.25.1/tests/fonts),
used under its [MIT license](LICENSE-MIT). It is only used by tests.
It has no naming table, so the test registers its family explicitly with fontdb.
