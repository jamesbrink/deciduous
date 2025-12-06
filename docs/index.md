---
layout: default
title: Home
---

<div style="display: flex; gap: 30px; align-items: flex-start; flex-wrap: wrap;">

<div style="flex: 1; min-width: 300px;">

# Losselot

**Audio forensics meets AI-assisted development.**

Detect fake lossless files. Every decision tracked in a queryable graph.

![Demo](demo.gif)

<p style="margin-top: 15px;">
<a href="analyzer.html" style="display: inline-block; background: #3b82f6; color: white; padding: 10px 20px; border-radius: 6px; text-decoration: none; font-weight: 600; margin-right: 10px;">Try in Browser</a>
<a href="demo/" style="display: inline-block; background: #16213e; color: #60a5fa; padding: 10px 20px; border-radius: 6px; text-decoration: none; font-weight: 600; border: 1px solid #3b82f6;">View Decision Graph</a>
</p>

</div>

<div style="flex: 1; min-width: 300px;">

## How It Works

When someone converts MP3 to FLAC, the removed frequencies don't come back:

- **Spectral** - FFT detects frequency cutoffs
- **Binary** - Finds encoder signatures (LAME, FFmpeg)
- **Combined** - Agreement increases confidence

| Score | Verdict | Meaning |
|:-----:|:-------:|:--------|
| 0-34 | OK | Clean |
| 35-64 | SUSPECT | Possibly transcoded |
| 65+ | TRANSCODE | Definitely lossy origin |

## Quick Start

```bash
git clone https://github.com/notactuallytreyanastasio/losselot
cd losselot && cargo build --release
./target/release/losselot serve ~/Music/
```

</div>

</div>

---

## The Living Museum

This project tracks every decision in a queryable graph. When context is lost, the reasoning survives.

<div style="display: flex; gap: 20px; flex-wrap: wrap; margin-top: 15px;">

<a href="decision-graph" style="flex: 1; min-width: 200px; padding: 15px; background: #16213e; border-radius: 8px; text-decoration: none; border: 1px solid #0f3460;">
<strong style="color: #4ade80;">Decision Graph</strong><br>
<span style="color: #999; font-size: 14px;">77+ nodes of dev decisions</span>
</a>

<a href="claude-tooling" style="flex: 1; min-width: 200px; padding: 15px; background: #16213e; border-radius: 8px; text-decoration: none; border: 1px solid #0f3460;">
<strong style="color: #60a5fa;">Claude Tooling</strong><br>
<span style="color: #999; font-size: 14px;">AI development workflow</span>
</a>

<a href="story" style="flex: 1; min-width: 200px; padding: 15px; background: #16213e; border-radius: 8px; text-decoration: none; border: 1px solid #0f3460;">
<strong style="color: #a855f7;">The Story</strong><br>
<span style="color: #999; font-size: 14px;">How this evolved</span>
</a>

</div>

<p style="margin-top: 20px; text-align: center;">
<a href="https://github.com/notactuallytreyanastasio/losselot">View on GitHub</a>
</p>
