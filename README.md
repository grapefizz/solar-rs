# solar-rs

A terminal-based solar system tracker and visualizer written in Rust.

`solar-rs` fetches real-time heliocentric ephemeris data (positions of planets in Astronomical Units (AU)) using the **NASA JPL Horizons API** and renders an interactive 2D map of the solar system directly in your terminal using [`ratatui`](https://github.com/ratatui-org/ratatui).

## Features

- **Real-Time Data**: Fetches precise heliocentric positions (X, Y, Z, and radial distance) of the Sun and eight planets.
- **Terminal Visualization**: Displays a table of the current coordinates and an interactive 2D orbital map.
- **Interactive Controls**: 
  - Zoom in/out of the map.
  - Change focus levels to fit different planets' orbits within the view.
- **Unicode Support**: Optional flag to use rich Unicode icons for planetary bodies.

## Prerequisites

- Standard Rust toolchain (Cargo and Rustc). [Install Rust](https://www.rust-lang.org/tools/install)

## Installation & Running

Clone the repository and run it using Cargo:

```bash
cargo run --release
```

To run with enhanced Unicode icons (recommended if your terminal supports them and you have a compatible font like Nerd Fonts installed):

```bash
cargo run --release -- --unicode
```

## Controls

While the application is running, you can interact with the map using the following keys:

- `+` / `=` : Zoom in.
- `-` : Zoom out.
- `0` : Reset zoom and focus to the default view (Neptune).
- `[` : Focus in (fit to smaller max orbit, e.g., inner planets).
- `]` : Focus out (fit to larger max orbit).
- `q` : Quit the application.

## How It Works

The app runs a background asynchronous task using `tokio` to fetch planetary vector data from the **NASA JPL Horizons API** for the current minute. The main thread listens to terminal input using `crossterm` and updates the `ratatui` UI at regular intervals.

## Dependencies

- **`ratatui`** & **`crossterm`**: Terminal UI layout and rendering.
- **`reqwest`** & **`tokio`**: Asynchronous HTTP pulling of the Horizons API.
- **`serde`** & **`serde_json`**: Parsing JSON responses.
- **`chrono`**: Time manipulation to request accurate current ephemeris.

## License

This project is licensed under the MIT or Apache 2.0 License.
