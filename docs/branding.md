# mai identity

mai adopts the Maida family **curly-brace symbol `{.}`**: two braces enclosing a coral dot. The braces connect it to code; the central dot and palette preserve the family resemblance while distinguishing mai from Maida's square-bracket mark. Use the existing geometry unchanged: the family mark gives mai a recognizable identity without depending on GitHub.

## Assets

- [Light surface](assets/mai-symbol-light.svg): ink braces with the coral dot.
- [Dark surface](assets/mai-symbol-dark.svg): paper braces with the coral dot.
- [OS-theme variant](assets/mai-symbol-dynamic.svg): a standalone SVG that follows `prefers-color-scheme`.

These files are unmodified copies of `svg/maida-symbol-curly-{light,dark,dynamic}.svg` from the sibling Maida Branding kit. They are vendored here so the repository renders without a sibling checkout. The upstream Branding masters remain unchanged. SVG titles retain their source family attribution; image alt text identifies mai where used.

Preserve the 144 × 128 viewBox, geometric paths, coral dot, and transparent background. Ink is `#171A17`, paper `#F2F1EB`, coral `#ED7958`; mint `#78D6A2` is a family accent. Do not add gradients, shadows, or a status meaning to the dot. The surface suffix names the intended background.

The README selects fixed light/dark images with `<picture>`, following [GitHub's responsive-image guidance](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/quickstart-for-writing-on-github). Use fixed variants when a UI controls its own theme. The dynamic file sees OS preferences, not a parent page's CSS variables or explicit theme attributes. Provide descriptive alt text.

## Front-page principles

Lead with the value, demonstrate a working outcome, make installation obvious, and link deeper details. Keep honest MVP limitations visible in a short paragraph. Avoid feature inventories, architecture essays, or invented performance claims on the front page. Test copyable examples against the executable.

The structure draws on [uv's value-led introduction and examples](https://github.com/astral-sh/uv), [ripgrep's quick documentation links and demonstrations](https://github.com/BurntSushi/ripgrep), and [Jujutsu's prominent onboarding links](https://github.com/jj-vcs/jj). These are design references, not proof that a particular README layout causes adoption. [GitHub's README guidance](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/customizing-your-repository/about-readmes) likewise prioritizes purpose, usefulness, getting started, and help.

[All docs →](README.md)
