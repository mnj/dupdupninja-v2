# dupdupninja-ui-gtk

GTK4 UI for Linux.

## Ubuntu 24.04 dependencies

Install system deps:

```bash
sudo apt update
sudo apt install -y libgtk-4-dev pkg-config
```

Build/run with the GTK implementation enabled:

```bash
cargo run -p dupdupninja-ui-gtk --features gtk
```
