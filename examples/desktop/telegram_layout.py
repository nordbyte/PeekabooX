#!/usr/bin/env python3
"""Locate Telegram UI targets in live desktop screenshots.

The helper intentionally keeps the detection lightweight and local to this
example. It prefers visual geometry over hard-coded absolute coordinates so the
desktop sample still works after the Telegram window is moved or resized.
"""

from __future__ import annotations

import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Callable, Iterable

try:
    from PIL import Image
except ImportError as exc:  # pragma: no cover - exercised by live environments
    raise SystemExit("python Pillow is required for Telegram layout detection") from exc


@dataclass(frozen=True)
class Component:
    pixels: int
    left: int
    top: int
    right: int
    bottom: int

    @property
    def center_x(self) -> int:
        return (self.left + self.right) // 2

    @property
    def center_y(self) -> int:
        return (self.top + self.bottom) // 2


def is_telegram_blue(pixel: tuple[int, int, int]) -> bool:
    red, green, blue = pixel
    return red <= 90 and 110 <= green <= 230 and 160 <= blue <= 255 and blue > green + 15


def is_telegram_panel(pixel: tuple[int, int, int]) -> bool:
    red, green, blue = pixel
    return (
        30 <= red <= 65
        and 35 <= green <= 75
        and 38 <= blue <= 85
        and green >= red - 7
        and blue >= green - 12
    )


def is_telegram_action(pixel: tuple[int, int, int]) -> bool:
    red, green, blue = pixel
    return red <= 100 and 120 <= green <= 230 and 120 <= blue <= 230 and green >= red + 50


def connected_components(
    image: Image.Image,
    predicate: Callable[[tuple[int, int, int]], bool],
    step: int = 2,
) -> Iterable[Component]:
    width, height = image.size
    grid_width = (width + step - 1) // step
    grid_height = (height + step - 1) // step
    mask = [
        [
            predicate(image.getpixel((min(x * step, width - 1), min(y * step, height - 1))))
            for x in range(grid_width)
        ]
        for y in range(grid_height)
    ]
    seen = [[False for _ in range(grid_width)] for _ in range(grid_height)]

    for start_y in range(grid_height):
        for start_x in range(grid_width):
            if seen[start_y][start_x] or not mask[start_y][start_x]:
                continue

            stack = [(start_x, start_y)]
            seen[start_y][start_x] = True
            pixels = 0
            min_x = max_x = start_x
            min_y = max_y = start_y

            while stack:
                x, y = stack.pop()
                pixels += 1
                min_x = min(min_x, x)
                max_x = max(max_x, x)
                min_y = min(min_y, y)
                max_y = max(max_y, y)

                for next_x, next_y in ((x + 1, y), (x - 1, y), (x, y + 1), (x, y - 1)):
                    if (
                        0 <= next_x < grid_width
                        and 0 <= next_y < grid_height
                        and not seen[next_y][next_x]
                        and mask[next_y][next_x]
                    ):
                        seen[next_y][next_x] = True
                        stack.append((next_x, next_y))

            yield Component(
                pixels=pixels * step * step,
                left=min_x * step,
                top=min_y * step,
                right=min((max_x + 1) * step, width),
                bottom=min((max_y + 1) * step, height),
            )


def locate_overview_icon(image: Image.Image) -> tuple[int, int]:
    width, height = image.size
    candidates = [
        component
        for component in connected_components(image, is_telegram_blue, step=2)
        if component.pixels >= 300
        and component.top >= int(height * 0.08)
        and component.bottom <= int(height * 0.65)
    ]
    if not candidates:
        raise SystemExit("could not locate Telegram result in GNOME overview")

    def score(component: Component) -> float:
        center_penalty = abs(component.center_x - (width / 2)) + abs(
            component.center_y - (height * 0.22)
        )
        return component.pixels - center_penalty

    target = max(candidates, key=score)
    return target.center_x, target.center_y


def horizontal_runs(
    image: Image.Image,
    y: int,
    predicate: Callable[[tuple[int, int, int]], bool],
    min_width: int,
) -> Iterable[tuple[int, int]]:
    start: int | None = None
    previous: int | None = None
    for x in range(image.width):
        if predicate(image.getpixel((x, y))):
            if start is None:
                start = previous = x
            else:
                previous = x
            continue

        if start is not None and previous is not None and previous - start + 1 >= min_width:
            yield start, previous
        start = previous = None

    if start is not None and previous is not None and previous - start + 1 >= min_width:
        yield start, previous


def locate_message_bar(image: Image.Image) -> tuple[int, int, int]:
    min_run_width = max(300, int(image.width * 0.25))
    candidates: list[tuple[int, int, int]] = []
    for y in range(int(image.height * 0.45), int(image.height * 0.95)):
        for left, right in horizontal_runs(image, y, is_telegram_panel, min_run_width):
            width = right - left + 1
            candidates.append((width, y, left, right))

    if not candidates:
        raise SystemExit("could not locate Telegram message bar")

    width, y, left, right = max(candidates, key=lambda item: (item[0], item[1]))
    return left, right, y


def locate_window_metrics(image: Image.Image) -> tuple[int, int, int, int]:
    bar_left, bar_right, bar_y = locate_message_bar(image)
    sidebar_width = 72
    left = max(0, bar_left - sidebar_width)
    right = bar_right
    bottom = min(image.height, bar_y + 5)

    top_candidates: list[int] = []
    min_run_width = max(300, int(image.width * 0.25))
    for y in range(0, max(1, int(image.height * 0.45))):
        for run_left, run_right in horizontal_runs(image, y, is_telegram_panel, min_run_width):
            if abs(run_right - right) <= 90 and run_left <= bar_left + 90:
                top_candidates.append(y)

    top = min(top_candidates) if top_candidates else max(0, bottom - 760)
    return left, top, right, bottom


def locate_search_input(image: Image.Image) -> tuple[int, int]:
    left, top, _right, _bottom = locate_window_metrics(image)
    return left + 250, top + 50


def locate_search_clear(image: Image.Image) -> tuple[int, int]:
    left, top, _right, _bottom = locate_window_metrics(image)
    return left + 380, top + 50


def locate_search_result(image: Image.Image) -> tuple[int, int]:
    left, top, _right, _bottom = locate_window_metrics(image)
    return left + 178, top + 108


def locate_message_input(image: Image.Image) -> tuple[int, int]:
    left, right, y = locate_message_bar(image)
    return left + int((right - left) * 0.42), max(0, y - 17)


def locate_send_button(image: Image.Image) -> tuple[int, int]:
    _left, right, y = locate_message_bar(image)
    return max(0, right - 24), max(0, y - 17)


def draft_send_button_active(image: Image.Image) -> bool:
    _left, right, y = locate_message_bar(image)
    left = max(0, right - 60)
    top = max(0, y - 45)
    bottom = min(image.height, y + 10)
    blue_pixels = 0
    for py in range(top, bottom):
        for px in range(left, right + 1):
            if is_telegram_action(image.getpixel((px, py))):
                blue_pixels += 1
    return blue_pixels >= 80


def load_image(path: Path) -> Image.Image:
    return Image.open(path).convert("RGB")


def main() -> int:
    commands = {
        "overview-icon",
        "search-input",
        "search-clear",
        "search-result",
        "message-input",
        "send-button",
        "draft-active",
    }
    if len(sys.argv) != 3 or sys.argv[1] not in commands:
        print(
            "usage: telegram_layout.py <overview-icon|search-input|search-clear|search-result|message-input|send-button|draft-active> <screenshot>",
            file=sys.stderr,
        )
        return 2

    command = sys.argv[1]
    image = load_image(Path(sys.argv[2]))
    if command == "overview-icon":
        x, y = locate_overview_icon(image)
    elif command == "search-input":
        x, y = locate_search_input(image)
    elif command == "search-clear":
        x, y = locate_search_clear(image)
    elif command == "search-result":
        x, y = locate_search_result(image)
    elif command == "message-input":
        x, y = locate_message_input(image)
    elif command == "draft-active":
        if draft_send_button_active(image):
            print("yes")
            return 0
        print("no")
        return 1
    else:
        x, y = locate_send_button(image)

    print(f"{x} {y}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
