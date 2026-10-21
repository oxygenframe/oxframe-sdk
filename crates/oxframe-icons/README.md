# oxframe-icons

`icon!("lucide:arrow-right")` for [oxframe](https://oxframe.dev). Used
through the `icons` feature of `oxframe`.

The macro reads the icon sets at compile time and puts only the SVG of the
icons you name into your binary.

| Prefix | Set | License |
| --- | --- | --- |
| `lucide:` | [Lucide](https://lucide.dev) | ISC (`data/LICENSE-lucide`) |
| `tabler:` | [Tabler Icons](https://tabler.io/icons); `-filled` names are the filled variants | MIT (`data/LICENSE-tabler`) |
| `simple:` | [Simple Icons](https://simpleicons.org), brand logos | CC0 (`data/LICENSE-simple-icons.md`); the brands themselves are trademarks of their owners |

`data/` is built by `scripts/build-icons.py` in the oxframe-sdk repository.
