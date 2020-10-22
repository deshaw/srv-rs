# {{crate}}

{{readme}}

## Usage

Add {{crate}} to your dependencies in `Cargo.toml`, enabling at least one of
the DNS resolver backends (see [Alternative Resolvers](README.md#alternative-resolvers-and-target-selection-policies)).
`libresolv` is enabled here as an example, but it is not required.

```toml
[dependencies]
{{crate}} = { git = "https://github.com/deshaw/{{crate}}", features = ["libresolv"] }
```

## Contributing

1. Clone the repo
2. Make some changes
3. Test: `cargo test`
4. Format: `cargo fmt`
5. Clippy: `cargo clippy`
6. Bench: `cargo bench`
7. If modifying crate-level docs (`src/lib.rs`) or `README.tpl`, update `README.md`:
    1. `cargo install cargo-readme`
    2. `cargo readme > README.md`

## History

This project was contributed back to the community by the [D. E. Shaw group](https://www.deshaw.com/).

<p align="center">
    <a href="https://www.deshaw.com">
       <img src="https://www.deshaw.com/assets/logos/black_logo_417x125.png" alt="D. E. Shaw Logo" height="75" >
    </a>
</p>
