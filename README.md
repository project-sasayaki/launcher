# Sasayaki Launcher

A Windows launcher for the Initial D series and Wangan Midnight Maximum Tune 3 / 3DX / 3DX+
arcade games. It identifies a game from its files, writes the configuration its loader needs,
and starts it:

| Loader | Games | Platform |
| --- | --- | --- |
| kitten | Initial D Arcade Stage 4 / 5 | Lindbergh |
| mice | Initial D Arcade Stage 6 / 7 / 8 | RingEdge |
| segatools | Initial D The Arcade, Initial D Arcade Stage Zero | ALLS / Nu |
| kitten | Wangan Midnight Maximum Tune 3 / 3DX / 3DX+ | Namco System N2 |

It also handles card selection, in-store versus play over a virtual LAN, an OpenVPN
connection, and automatic updates of the launcher and its tools.

## Requirements

- Windows with the [Windows App SDK runtime](https://learn.microsoft.com/windows/apps/windows-app-sdk/downloads) 2.x
- The loaders and tools placed under `tools\` next to `launcher.exe`
  (see [Placing the tools](docs/design.md#1-placing-the-tools-important))

## Building

Needs the Rust toolchain (MSVC target) and the Visual Studio C++ build tools.

```bash
cargo build --release   # target/release/launcher.exe
cargo test              # unit tests
```

## Documentation

[docs/design.md](docs/design.md) describes how every game is launched, the configuration each
loader receives, the interface, the data files and the source layout.
