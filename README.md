# herdr (personal fork)

<p align="center">
  <img src="assets/logo.png" alt="herdr" width="100" />
</p>

<p align="center">
  <a href="https://herdr.dev">herdr.dev</a> · <a href="#install">install</a> · <a href="https://herdr.dev/docs/quick-start/">quick start</a> · <a href="https://herdr.dev/docs/">docs</a>
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-Apache--2.0-666666?labelColor=333333" alt="Apache 2.0 license" /></a>
</p>

---

https://github.com/user-attachments/assets/043ec09f-4bdd-41d5-aee0-8fda6b83e267

**agent multiplexer that lives in your terminal.**

- **every agent at a glance** — blocked, working, done. real terminal views, not a wrapped interpretation.
- **detach, agents keep running** — reattach from any terminal, or over ssh. sessions survive restarts.
- **agents can use herdr too** — a pure socket api: agents spawn panes, read output, wait on each other. [agent skill →](https://herdr.dev/docs/agent-skill/)
- **keyboard and mouse, both first-class** — tmux-style prefix keys *and* click, drag, split. pick per moment, not per tool.
- **plugins** — extend panes and workflows. [browse the marketplace →](https://herdr.dev/plugins/)
- **one rust binary, no electron** — runs in whatever terminal you already use.

---

This is a personal fork of [ogulcancelik/herdr](https://github.com/ogulcancelik/herdr) with
Windows-focused customizations. For the official release, install from upstream.

## install

**Linux / macOS:**

```bash
curl -fsSL https://raw.githubusercontent.com/PaRr0tBoY/herdr/master/scripts/install.sh | sh
```

**Windows (PowerShell):**

```powershell
irm https://raw.githubusercontent.com/PaRr0tBoY/herdr/master/scripts/install.ps1 | iex
```

Or download binaries directly from [releases](https://github.com/PaRr0tBoY/herdr/releases).

Then start it where the work lives:

```bash
herdr
```

run your agents, split panes, walk away. `ctrl+b q` detaches, `herdr` reattaches. [quick start →](https://herdr.dev/docs/quick-start/)

## docs

everything lives at [herdr.dev/docs](https://herdr.dev/docs/): [quick start](https://herdr.dev/docs/quick-start/) · [concepts](https://herdr.dev/docs/concepts/) · [supported agents](https://herdr.dev/docs/agents/) · [keyboard](https://herdr.dev/docs/keyboard/) · [configuration](https://herdr.dev/docs/configuration/) · [session state](https://herdr.dev/docs/session-state/) · [remote](https://herdr.dev/docs/persistence-remote/) · [integrations](https://herdr.dev/docs/integrations/) · [plugins](https://herdr.dev/docs/plugins/) · [socket api](https://herdr.dev/docs/socket-api/)

## agent instructions

if you are an ai agent helping with this repository, read [`AGENTS.md`](./AGENTS.md) before making changes and read [`CONTRIBUTING.md`](./CONTRIBUTING.md) before opening issues or PRs.

## development

```bash
git clone https://github.com/PaRr0tBoY/herdr
cd herdr
git remote add upstream https://github.com/ogulcancelik/herdr
cargo build --release

cargo test       # unit tests
cargo fmt -- --check  # formatting
```

## license

Herdr is licensed under the [Apache License 2.0](LICENSE).
