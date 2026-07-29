# Hive

<p align="center">
  <img src="assets/logo.svg" alt="herdr" width="100" />
</p>

<p align="center">
  <a href="https://herdr.dev">herdr.dev</a> · <a href="#install">install</a> · <a href="https://herdr.dev/docs/quick-start/">quick start</a> · <a href="https://herdr.dev/docs/">docs</a>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-666666?labelColor=333333" alt="Apache 2.0 license" /></a>
</p>

---

## **Herdr Fork that keeps you in flow**

> This is a personal fork of [ogulcancelik/herdr](https://github.com/ogulcancelik/herdr) with
> Windows-focused customizations. For the official release, install from upstream.

- Embedded Note Editor per space that always keep your notes

- The Tab Launcher that help your make decisions.

- Tab Auto Rename with agents' session name

- Fixed Shift Enter next line

- Optimized For Windows

---

## install

**Linux / macOS:**

```bash
curl -fsSL https://PaRr0tBoY.github.io/product/Hive/install/install.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://PaRr0tBoY.github.io/product/Hive/install/install.ps1 | iex
```

Or download binaries directly from [releases](https://github.com/PaRr0tBoY/hive/releases).

Then start it where the work lives:

```bash
hive
```

run your agents, split panes, walk away. `ctrl+b q` detaches, `hive` reattaches. [quick start →](https://herdr.dev/docs/quick-start/)

## docs

everything lives at [herdr.dev/docs](https://herdr.dev/docs/): [quick start](https://herdr.dev/docs/quick-start/) · [concepts](https://herdr.dev/docs/concepts/) · [supported agents](https://herdr.dev/docs/agents/) · [keyboard](https://herdr.dev/docs/keyboard/) · [configuration](https://herdr.dev/docs/configuration/) · [session state](https://herdr.dev/docs/session-state/) · [remote](https://herdr.dev/docs/persistence-remote/) · [integrations](https://herdr.dev/docs/integrations/) · [plugins](https://herdr.dev/docs/plugins/) · [socket api](https://herdr.dev/docs/socket-api/)

## development

```bash
git clone https://github.com/PaRr0tBoY/hive
cd hive
git remote add upstream https://github.com/ogulcancelik/herdr
cargo build --release

cargo test       # unit tests
cargo fmt -- --check  # formatting
```

## license

hive is licensed under the [Apache License 2.0](LICENSE).
