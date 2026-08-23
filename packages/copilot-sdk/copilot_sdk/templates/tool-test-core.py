import unittest

from __PYTHON_PACKAGE__.core import analyze_record


class AnalyzeRecordTests(unittest.TestCase):
    def test_preserves_typed_record(self) -> None:
        self.assertEqual(
            analyze_record("sample", 7),
            {"status": "ok", "record_id": "sample", "value": 7},
        )

    def test_rejects_empty_record_id(self) -> None:
        with self.assertRaises(ValueError):
            analyze_record("  ", 7)


if __name__ == "__main__":
    unittest.main()
