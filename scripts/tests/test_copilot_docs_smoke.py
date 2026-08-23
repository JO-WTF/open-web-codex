from __future__ import annotations

import tempfile
import unittest
from pathlib import Path

from scripts.copilot_docs_smoke import (
    DocsSmokeError,
    check_markdown_links,
    github_anchors,
)


class MarkdownLinkContractTests(unittest.TestCase):
    def test_validates_local_files_and_unicode_anchors(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / "target.md"
            target.write_text(
                "# 测试与排错\n\n## Runtime 二进制缺失\n\n## Runtime 二进制缺失\n",
                encoding="utf-8",
            )
            source = root / "source.md"
            source.write_text(
                "[first](target.md#runtime-二进制缺失)\n"
                "[duplicate](target.md#runtime-二进制缺失-1)\n",
                encoding="utf-8",
            )

            self.assertEqual(check_markdown_links([source]), 2)
            self.assertIn("runtime-二进制缺失", github_anchors(target))

    def test_ignores_links_inside_code_fences(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source.md"
            source.write_text(
                "```markdown\n[not a link](missing.md)\n```\n", encoding="utf-8"
            )

            self.assertEqual(check_markdown_links([source]), 0)

    def test_reports_missing_target_with_source_line(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "source.md"
            source.write_text("# Source\n\n[broken](missing.md)\n", encoding="utf-8")

            with self.assertRaisesRegex(DocsSmokeError, "source.md:3"):
                check_markdown_links([source])


if __name__ == "__main__":
    unittest.main()
