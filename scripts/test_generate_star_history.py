import unittest
import xml.etree.ElementTree as ElementTree
from datetime import date

from scripts.generate_star_history import (
    history_points,
    nice_axis_max,
    render_star_history_svg,
    validate_repository,
)


class StarHistoryTests(unittest.TestCase):
    def test_renders_aggregated_history_without_user_data(self):
        dates = [
            date(2023, 7, 2),
            date(2023, 7, 2),
            date(2024, 1, 10),
            date(2025, 8, 25),
        ]
        svg = render_star_history_svg("tsonglew/dutis", dates, date(2026, 8, 25))

        self.assertIn("Dutis · Star History", svg)
        self.assertIn("★ 4", svg)
        self.assertIn("tsonglew/dutis", svg)
        self.assertIn("25 Aug 2026", svg)
        self.assertNotIn("login", svg)
        ElementTree.fromstring(svg)

    def test_builds_monotonic_daily_points(self):
        start, end, points = history_points(
            [date(2024, 1, 1), date(2024, 1, 1), date(2024, 1, 3)],
            date(2024, 1, 5),
        )

        self.assertEqual(start, date(2024, 1, 1))
        self.assertEqual(end, date(2024, 1, 5))
        self.assertEqual(
            points,
            [
                (date(2024, 1, 1), 2),
                (date(2024, 1, 3), 3),
                (date(2024, 1, 5), 3),
            ],
        )

    def test_empty_history_and_axis_are_renderable(self):
        start, end, points = history_points([], date(2026, 8, 25))
        self.assertEqual((end - start).days, 30)
        self.assertEqual(points, [(start, 0), (end, 0)])
        self.assertEqual(nice_axis_max(0), 4)
        self.assertEqual(nice_axis_max(17), 20)

    def test_repository_validation_rejects_urls_and_path_traversal(self):
        self.assertEqual(validate_repository("tsonglew/dutis"), "tsonglew/dutis")
        for invalid in ("https://github.com/tsonglew/dutis", "../dutis", "owner/repo/x"):
            with self.subTest(invalid=invalid):
                with self.assertRaises(ValueError):
                    validate_repository(invalid)


if __name__ == "__main__":
    unittest.main()
