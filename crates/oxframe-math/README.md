# oxframe-math

Formulas for [oxframe](https://oxframe.dev), set by
[Typst](https://typst.app). Used through the `math` feature of `oxframe`:

```toml
oxframe = { version = "0.21", features = ["math"] }
```

```rust
let euler = view.add(Math::new("e^(i pi) + 1 = 0").size(120.0));
stagger(0.08, euler.paths(), |glyph| glyph.pop_in()).await;
```

The formula is laid out with the bundled New Computer Modern Math font, so
it looks the same on every machine and needs no TeX install. Each glyph and
rule becomes a path. The feature adds the Typst compiler and a 1.4 MB font
to your binary; it is not available in the browser playground.

| Data | License |
| --- | --- |
| Code | MIT OR Apache-2.0 |
| `fonts/NewCMMath-Book.otf` (New Computer Modern Math) | GUST Font License, based on LPPL 1.3c (`fonts/LICENSE-GUST.txt`) |
