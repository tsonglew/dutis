#!/usr/bin/env python3
"""Generate a self-hosted SVG star-history chart from GitHub's API."""

from __future__ import annotations

import argparse
import html
import json
import math
import os
import re
import sys
import urllib.error
import urllib.request
from collections import Counter
from datetime import UTC, date, datetime, timedelta
from pathlib import Path


API_VERSION = "2022-11-28"
REPOSITORY_PATTERN = re.compile(r"^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$")


def validate_repository(repository: str) -> str:
    repository = repository.strip()
    if not REPOSITORY_PATTERN.fullmatch(repository):
        raise ValueError("repository must use the owner/name form")
    owner, name = repository.split("/", maxsplit=1)
    if owner in {".", ".."} or name in {".", ".."}:
        raise ValueError("repository owner and name must not be path segments")
    return repository


def fetch_stargazer_dates(repository: str, token: str) -> list[date]:
    repository = validate_repository(repository)
    if not token.strip():
        raise ValueError("GITHUB_TOKEN is required")

    dates: list[date] = []
    page = 1
    while True:
        url = (
            f"https://api.github.com/repos/{repository}/stargazers"
            f"?per_page=100&page={page}"
        )
        request = urllib.request.Request(
            url,
            headers={
                "Accept": "application/vnd.github.star+json",
                "Authorization": f"Bearer {token}",
                "User-Agent": "dutis-star-history",
                "X-GitHub-Api-Version": API_VERSION,
            },
        )
        try:
            with urllib.request.urlopen(request, timeout=30) as response:
                payload = json.load(response)
        except urllib.error.HTTPError as error:
            detail = error.read().decode("utf-8", errors="replace")
            raise RuntimeError(
                f"GitHub stargazers request failed with HTTP {error.code}: {detail}"
            ) from error

        if not isinstance(payload, list):
            raise RuntimeError("GitHub stargazers response was not a list")
        for item in payload:
            starred_at = item.get("starred_at") if isinstance(item, dict) else None
            if not isinstance(starred_at, str):
                raise RuntimeError(
                    "GitHub did not return star timestamps; verify repository permissions"
                )
            dates.append(datetime.fromisoformat(starred_at.replace("Z", "+00:00")).date())
        if len(payload) < 100:
            break
        page += 1

    return sorted(dates)


def nice_axis_max(value: int) -> int:
    if value <= 4:
        return 4
    magnitude = 10 ** math.floor(math.log10(value))
    for multiplier in (1, 2, 5, 10):
        candidate = multiplier * magnitude
        if candidate >= value:
            return candidate
    return value


def history_points(star_dates: list[date], generated_on: date) -> tuple[date, date, list[tuple[date, int]]]:
    if not star_dates:
        start = generated_on - timedelta(days=30)
        return start, generated_on, [(start, 0), (generated_on, 0)]

    counts = Counter(star_dates)
    start = min(star_dates)
    end = max(generated_on, max(star_dates), start + timedelta(days=1))
    cumulative = 0
    points: list[tuple[date, int]] = []
    for day in sorted(counts):
        cumulative += counts[day]
        points.append((day, cumulative))
    if points[-1][0] != end:
        points.append((end, cumulative))
    return start, end, points


def render_star_history_svg(
    repository: str, star_dates: list[date], generated_on: date
) -> str:
    repository = validate_repository(repository)
    width, height = 960, 480
    left, right, top, bottom = 72, 36, 92, 62
    plot_width = width - left - right
    plot_height = height - top - bottom
    start, end, points = history_points(star_dates, generated_on)
    day_span = max((end - start).days, 1)
    y_max = nice_axis_max(len(star_dates))

    def x_position(day: date) -> float:
        return left + ((day - start).days / day_span) * plot_width

    def y_position(value: int) -> float:
        return top + plot_height - (value / y_max) * plot_height

    line_points = " ".join(
        f"{x_position(day):.1f},{y_position(value):.1f}" for day, value in points
    )
    area_points = (
        f"{left},{top + plot_height} {line_points} "
        f"{x_position(points[-1][0]):.1f},{top + plot_height}"
    )

    y_grid = []
    for index in range(5):
        value = round(y_max * index / 4)
        y = y_position(value)
        y_grid.append(
            f'<line class="grid" x1="{left}" y1="{y:.1f}" x2="{width - right}" y2="{y:.1f}"/>'
            f'<text class="axis-label" x="{left - 14}" y="{y + 4:.1f}" text-anchor="end">{value}</text>'
        )

    x_labels = []
    used_dates: set[date] = set()
    for index in range(5):
        tick = start + timedelta(days=round(day_span * index / 4))
        if tick in used_dates:
            continue
        used_dates.add(tick)
        x = x_position(tick)
        x_labels.append(
            f'<text class="axis-label" x="{x:.1f}" y="{height - 28}" text-anchor="middle">'
            f"{html.escape(tick.strftime('%b %Y'))}</text>"
        )

    escaped_repository = html.escape(repository)
    updated = html.escape(generated_on.strftime("%d %b %Y"))
    last_x = x_position(points[-1][0])
    last_y = y_position(points[-1][1])
    return f'''<svg xmlns="http://www.w3.org/2000/svg" role="img" aria-labelledby="title description" viewBox="0 0 {width} {height}">
  <title id="title">{escaped_repository} star history</title>
  <desc id="description">{len(star_dates)} GitHub stars through {updated}</desc>
  <defs>
    <linearGradient id="area" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" stop-color="#f04b2f" stop-opacity="0.32"/>
      <stop offset="1" stop-color="#f04b2f" stop-opacity="0.03"/>
    </linearGradient>
  </defs>
  <style>
    .background {{ fill: #fffdf7; }}
    .grid {{ stroke: #d8d2c5; stroke-width: 1; }}
    .axis-label {{ fill: #6f6b60; font: 12px ui-monospace, SFMono-Regular, Menlo, monospace; }}
    .title {{ fill: #171712; font: 700 24px ui-sans-serif, system-ui, sans-serif; }}
    .subtitle {{ fill: #6f6b60; font: 13px ui-monospace, SFMono-Regular, Menlo, monospace; }}
    .count {{ fill: #171712; font: 700 32px ui-sans-serif, system-ui, sans-serif; }}
    .line {{ fill: none; stroke: #f04b2f; stroke-linecap: round; stroke-linejoin: round; stroke-width: 4; }}
    .area {{ fill: url(#area); }}
    .last {{ fill: #fffdf7; stroke: #f04b2f; stroke-width: 4; }}
    @media (prefers-color-scheme: dark) {{
      .background {{ fill: #171712; }}
      .grid {{ stroke: #3d3b35; }}
      .axis-label, .subtitle {{ fill: #aaa59a; }}
      .title, .count {{ fill: #fffdf7; }}
      .last {{ fill: #171712; }}
    }}
  </style>
  <rect class="background" width="{width}" height="{height}" rx="16"/>
  <text class="title" x="{left}" y="42">Dutis · Star History</text>
  <text class="subtitle" x="{left}" y="67">{escaped_repository} · updated {updated}</text>
  <text class="count" x="{width - right}" y="48" text-anchor="end">★ {len(star_dates)}</text>
  {''.join(y_grid)}
  <polygon class="area" points="{area_points}"/>
  <polyline class="line" points="{line_points}"/>
  <circle class="last" cx="{last_x:.1f}" cy="{last_y:.1f}" r="6"/>
  {''.join(x_labels)}
</svg>
'''


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repository", required=True, help="GitHub owner/name")
    parser.add_argument("--output", required=True, type=Path, help="SVG output path")
    args = parser.parse_args()

    try:
        dates = fetch_stargazer_dates(
            args.repository, os.environ.get("GITHUB_TOKEN", "")
        )
        svg = render_star_history_svg(args.repository, dates, datetime.now(UTC).date())
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(svg, encoding="utf-8")
    except (OSError, RuntimeError, ValueError) as error:
        print(f"error: {error}", file=sys.stderr)
        return 1

    print(f"generated {args.output} from {len(dates)} stars")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
