"""Bounded Workspace source discovery and evidence profiling.

This module deliberately exposes only structural samples to the model.  Full
file reads are reserved for the deterministic normalizer after the user has
confirmed the mapping after the requirement profile was published.
"""

from __future__ import annotations

import csv
import hashlib
import io
import itertools
import json
import re
import stat
import zipfile
from pathlib import Path, PurePosixPath
from typing import Any, BinaryIO
from xml.etree import ElementTree

import ijson
from charset_normalizer import from_bytes
from openpyxl import load_workbook

MAX_FILES = 500
MAX_BYTES = 100 * 1024 * 1024
MAX_SAMPLE_ROWS = 20
MAX_JSON_DEPTH = 64
MAX_JSON_NODES = 250_000
MAX_JSON_STRING = 64 * 1024
MAX_XLSX_SHEETS = 32
MAX_XLSX_COLUMNS = 256
MAX_NORMALIZE_ROWS = 1_000_000
EXCLUDED_DIRS = {
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".venv",
    ".demo-staging",
    "__pycache__",
    "datasets",
}
SUPPORTED_SUFFIXES = {".xlsx", ".csv", ".json"}
DEMO_MANIFEST_SCHEMA = "demo_workspace_sources.v1"
GENERATED_OUTPUT_ROOT = PurePosixPath("outputs/warehouse-network")


def _is_generated_output(relative: Path) -> bool:
    parts = relative.parts
    return len(parts) >= 2 and parts[:2] == GENERATED_OUTPUT_ROOT.parts


def _iter_files(root: Path) -> list[Path]:
    root = root.resolve(strict=True)
    files: list[Path] = []
    for path in root.rglob("*"):
        relative = path.relative_to(root)
        if any(part in EXCLUDED_DIRS for part in relative.parts) or _is_generated_output(relative):
            continue
        if path.is_symlink():
            if path.suffix.lower() in SUPPORTED_SUFFIXES | {".xls", ".xlsm"}:
                raise ValueError("workspace_source_symlink_rejected")
            continue
        if path.is_file() and path.suffix.lower() in SUPPORTED_SUFFIXES:
            files.append(path)
    files.sort(key=lambda item: item.relative_to(root).as_posix())
    return files


def _validated_source_path(root: Path, relative_path: str) -> Path:
    """Resolve one model-visible Workspace path without following symlinks."""
    canonical_root = root.resolve(strict=True)
    if not canonical_root.is_dir():
        raise ValueError("workspace_root_not_directory")
    if not isinstance(relative_path, str) or not relative_path:
        raise ValueError("workspace_relative_path_required")
    relative = PurePosixPath(relative_path)
    if (
        relative.is_absolute()
        or not relative.parts
        or relative.as_posix() != relative_path
        or any(part in {"", ".", ".."} for part in relative.parts)
        or any(part in EXCLUDED_DIRS for part in relative.parts)
    ):
        raise ValueError("invalid_workspace_relative_path")

    current = canonical_root
    for index, part in enumerate(relative.parts):
        current = current / part
        try:
            mode = current.lstat().st_mode
        except FileNotFoundError as error:
            raise ValueError("workspace_source_not_found") from error
        if stat.S_ISLNK(mode):
            raise ValueError("workspace_source_symlink_rejected")
        if index < len(relative.parts) - 1 and not stat.S_ISDIR(mode):
            raise ValueError("workspace_source_parent_not_directory")
    if not stat.S_ISREG(current.lstat().st_mode):
        raise ValueError("workspace_source_not_regular_file")
    try:
        current.resolve(strict=True).relative_to(canonical_root)
    except ValueError as error:
        raise ValueError("workspace_source_escape_rejected") from error
    if current.suffix.lower() not in SUPPORTED_SUFFIXES:
        raise ValueError("unsupported_source_format")
    size = current.stat().st_size
    if size <= 0 or size > MAX_BYTES:
        raise ValueError("workspace_source_size_limit")
    return current


def source_descriptor(root: Path, relative_path: str) -> dict[str, Any]:
    """Return the stable public descriptor for one validated Workspace file."""
    path = _validated_source_path(root, relative_path)
    return {
        "relative_path": relative_path,
        "format": path.suffix.lower().removeprefix("."),
        "size": path.stat().st_size,
    }


def discover(root: Path) -> list[dict[str, Any]]:
    legacy = [
        path
        for path in root.rglob("*")
        if not any(part in EXCLUDED_DIRS for part in path.relative_to(root).parts)
        and not _is_generated_output(path.relative_to(root))
        and path.is_file()
        and path.suffix.lower() in {".xls", ".xlsm"}
    ]
    if legacy:
        raise ValueError(
            "unsupported_source_format: legacy .xls/.xlsm files must be exported "
            "as .xlsx, .csv or .json"
        )
    files = _iter_files(root)
    if len(files) > MAX_FILES:
        raise ValueError("workspace_source_limit_exceeded: more than 500 supported files")
    return [source_descriptor(root, path.relative_to(root).as_posix()) for path in files]


def workspace_source_metadata(root: Path) -> dict[str, Any]:
    """Return verified source classification without trusting a filename alone."""
    manifests = sorted(root.glob("demo-data/*/demo-manifest.json"))
    if not manifests:
        return {"dataClassification": "workspace_data"}
    if len(manifests) != 1 or manifests[0].is_symlink():
        raise ValueError("demo_manifest_conflict")
    manifest_path = manifests[0]
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError("demo_manifest_invalid") from error
    if (
        manifest.get("schemaVersion") != DEMO_MANIFEST_SCHEMA
        or manifest.get("dataClassification") != "synthetic_demo"
        or not isinstance(manifest.get("template"), dict)
        or not isinstance(manifest.get("files"), list)
    ):
        raise ValueError("demo_manifest_invalid")
    expected_names: set[str] = set()
    for item in manifest["files"]:
        if not isinstance(item, dict):
            raise ValueError("demo_manifest_invalid")
        name = item.get("displayName")
        expected_size = item.get("bytes")
        expected_digest = item.get("contentSha256")
        if (
            not isinstance(name, str)
            or Path(name).name != name
            or name in expected_names
            or not isinstance(expected_size, int)
            or not isinstance(expected_digest, str)
            or not re.fullmatch(r"[0-9a-f]{64}", expected_digest)
        ):
            raise ValueError("demo_manifest_invalid")
        expected_names.add(name)
        source = manifest_path.parent / name
        if source.is_symlink() or not source.is_file():
            raise ValueError("demo_manifest_source_missing")
        if source.stat().st_size != expected_size or _sha256_file(source) != expected_digest:
            raise ValueError("demo_manifest_source_mismatch")
    actual_names = {
        path.name
        for path in manifest_path.parent.iterdir()
        if path.is_file() and path.name != manifest_path.name
    }
    if actual_names != expected_names:
        raise ValueError("demo_manifest_source_mismatch")
    template = manifest["template"]
    return {
        "dataClassification": "synthetic_demo",
        "demoTemplate": {
            "id": template.get("id"),
            "version": template.get("version"),
            "seed": template.get("seed"),
        },
    }


def _sha256_file(path: Path) -> str:
    """Verify explicit Demo fixture bytes without exposing a source identity."""
    digest = hashlib.sha256()
    with path.open("rb") as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def inspect(root: Path, relative_path: str) -> dict[str, Any]:
    path = _validated_source_path(root, relative_path)
    record = source_descriptor(root, relative_path)
    if path.suffix.lower() == ".csv":
        structure = inspect_csv(path)
    elif path.suffix.lower() == ".json":
        structure = inspect_json(path)
    else:
        structure = inspect_xlsx(path)
    return {**record, "structure": structure}


def inspect_csv(path: Path) -> dict[str, Any]:
    raw = _read_prefix(path, 256 * 1024)
    text = decode_text(raw)
    sample = text.splitlines()[: MAX_SAMPLE_ROWS + 1]
    if not sample:
        return {
            "kind": "table",
            "columns": [],
            "preview": _preview_payload([], total_count=0, total_count_exact=True),
            "record_count": 0,
            "record_count_exact": True,
        }
    try:
        dialect = csv.Sniffer().sniff("\n".join(sample[:10]), delimiters=",;\t|")
        delimiter = dialect.delimiter
    except csv.Error:
        delimiter = ","
    rows = list(csv.reader(sample, delimiter=delimiter))
    if len(rows[0]) > MAX_XLSX_COLUMNS:
        raise ValueError("source_column_limit_exceeded")
    header = [cell.strip() for cell in rows[0]]
    preview_rows = rows[1 : MAX_SAMPLE_ROWS + 1]
    if any(len(row) > MAX_XLSX_COLUMNS for row in preview_rows):
        raise ValueError("source_column_limit_exceeded")
    record_count = _count_csv_records(path, delimiter)
    if record_count > MAX_NORMALIZE_ROWS:
        raise ValueError("source_row_limit_exceeded")
    return {
        "kind": "table",
        "delimiter": delimiter,
        "columns": header,
        "preview": _preview_payload(
            preview_rows,
            total_count=record_count,
            total_count_exact=True,
        ),
        "record_count": record_count,
        "record_count_exact": True,
    }


def _preview_payload(
    rows: list[Any],
    *,
    total_count: int = 0,
    total_count_exact: bool = False,
) -> dict[str, Any]:
    return {
        "strategy": "head",
        "limit": MAX_SAMPLE_ROWS,
        "preview_sample_count": len(rows),
        "total_count": total_count,
        "total_count_exact": total_count_exact,
        "complete": False,
        "rows": rows,
    }


def _count_csv_records(path: Path, delimiter: str) -> int:
    with path.open("rb") as binary:
        text_stream = io.TextIOWrapper(
            binary,
            encoding=detect_encoding(_read_prefix(path, 64 * 1024)),
            errors="strict",
        )
        reader = csv.reader(text_stream, delimiter=delimiter)
        next(reader, None)
        return sum(1 for row in reader if any(cell not in (None, "") for cell in row))


def inspect_json(path: Path) -> dict[str, Any]:
    arrays: dict[str, dict[str, Any]] = {}
    object_keys: dict[str, set[str]] = {}
    node_count = 0
    root_kind = "unknown"
    with path.open("rb") as stream:
        for prefix, event, value in _iter_json_events(stream):
            node_count += 1
            if node_count > MAX_JSON_NODES:
                raise ValueError("json_node_limit_exceeded")
            depth = _json_depth(prefix)
            if depth > MAX_JSON_DEPTH:
                raise ValueError("json_depth_limit_exceeded")
            if event == "start_map" and not prefix:
                root_kind = "object"
            elif event == "start_array" and not prefix:
                root_kind = "array"
            if event == "string" and isinstance(value, str) and len(value) > MAX_JSON_STRING:
                raise ValueError("json_string_limit_exceeded")
            if event == "start_array":
                array_key = prefix or "$"
                arrays[array_key] = {
                    "path": "$" if array_key == "$" else f"$.{array_key.replace('.item', '')}",
                    "length": 0,
                    "length_exact": True,
                    "preview": _preview_payload([]),
                }
            if event == "map_key":
                object_keys.setdefault(prefix, set()).add(str(value)[:256])
                if len(object_keys[prefix]) > MAX_XLSX_COLUMNS:
                    raise ValueError("source_column_limit_exceeded")
            if len(arrays) > 256 or len(object_keys) > 256:
                raise ValueError("json_structure_limit_exceeded")

    # ijson's event stream is used above for limits and structure.  A second
    # streaming pass counts every item while extracting at most three preview
    # items per discovered array.  The full JSON document is never loaded into
    # memory or sent to the model.
    for array_prefix, array in arrays.items():
        item_prefix = "item" if array_prefix == "$" else f"{array_prefix}.item"
        with path.open("rb") as stream:
            try:
                for index, item in enumerate(ijson.items(stream, item_prefix)):
                    if index >= MAX_NORMALIZE_ROWS:
                        raise ValueError("source_row_limit_exceeded")
                    array["length"] = index + 1
                    if index < 3:
                        if isinstance(item, dict):
                            array["preview"]["rows"].append(
                                {
                                    "kind": "object",
                                    "fields": {
                                        str(key)[:256]: {
                                            "type": type(value).__name__,
                                            "sample": _bounded_json_sample(value),
                                        }
                                        for key, value in item.items()
                                    },
                                }
                            )
                        else:
                            array["preview"]["rows"].append(
                                {
                                    "kind": "value",
                                    "type": type(item).__name__,
                                    "sample": _bounded_json_sample(item),
                                }
                            )
                    array["preview"]["preview_sample_count"] = len(array["preview"]["rows"])
            except (ijson.common.IncompleteJSONError, ijson.common.JSONError) as error:
                raise ValueError("invalid_json") from error
        array["preview"]["total_count"] = array["length"]
        array["preview"]["total_count_exact"] = array["length_exact"]
    return {
        "kind": "json",
        "tree": {"kind": root_kind},
        "object_keys": {path: sorted(keys) for path, keys in object_keys.items()},
        "arrays": list(arrays.values()),
        "node_count": node_count,
    }


def _bounded_json_sample(value: Any) -> Any:
    if isinstance(value, str):
        return value[:256]
    if isinstance(value, (int, float, bool)) or value is None:
        return value
    if isinstance(value, list):
        return [_bounded_json_sample(item) for item in value[:3]]
    if isinstance(value, dict):
        return {
            str(key)[:128]: _bounded_json_sample(item) for key, item in list(value.items())[:16]
        }
    return str(value)[:256]


def inspect_xlsx(path: Path) -> dict[str, Any]:
    with zipfile.ZipFile(path) as archive:
        infos = archive.infolist()
        if len(infos) > 2048:
            raise ValueError("xlsx_entry_limit_exceeded")
        expanded = sum(info.file_size for info in infos)
        if expanded > 512 * 1024 * 1024:
            raise ValueError("xlsx_expanded_size_limit_exceeded")
        if any(
            info.file_size and info.compress_size and info.file_size / info.compress_size > 100
            for info in infos
        ):
            raise ValueError("xlsx_compression_ratio_limit_exceeded")
        names = {info.filename for info in infos}
        if "xl/vbaProject.bin" in names or any(
            name.startswith("xl/externalLinks/") or name.endswith("externalLink1.xml")
            for name in names
        ):
            raise ValueError("unsupported_source_security_feature")
        _xlsx_shared_strings(archive)
    try:
        workbook = load_workbook(
            path,
            read_only=True,
            data_only=False,
            keep_links=False,
        )
    except Exception as error:
        # Do not echo the Runtime Workspace path into an Agent message.
        raise ValueError(f"xlsx_parse_failed:{type(error).__name__}") from error
    summaries = []
    try:
        if len(workbook.worksheets) > MAX_XLSX_SHEETS:
            raise ValueError("xlsx_sheet_limit_exceeded")
        for worksheet in workbook.worksheets:
            header: list[Any] | None = None
            preview_rows: list[list[Any]] = []
            record_count = 0
            for row in worksheet.iter_rows(values_only=False):
                values = []
                if len(row) > MAX_XLSX_COLUMNS:
                    raise ValueError("source_column_limit_exceeded")
                for cell in row:
                    value = cell.value
                    if isinstance(value, str) and value.startswith("="):
                        value = {"formula": True, "display": value[:MAX_JSON_STRING]}
                    values.append(value)
                if any(value not in (None, "") for value in values):
                    if header is None:
                        header = values
                    else:
                        record_count += 1
                        if record_count > MAX_NORMALIZE_ROWS:
                            raise ValueError("source_row_limit_exceeded")
                        if len(preview_rows) < MAX_SAMPLE_ROWS:
                            preview_rows.append(values)
            if header is not None:
                summaries.append(
                    {
                        "sheet": worksheet.title,
                        "columns": [str(value or "").strip() for value in header],
                        "preview": _preview_payload(
                            preview_rows,
                            total_count=record_count,
                            total_count_exact=True,
                        ),
                        "record_count": record_count,
                        "record_count_exact": True,
                        "preview_formula_cells": sum(
                            1
                            for row in [header, *preview_rows]
                            for value in row
                            if isinstance(value, dict) and value.get("formula") is True
                        ),
                    }
                )
    finally:
        workbook.close()
    return {"kind": "workbook", "sheets": summaries}


def _xlsx_shared_strings(archive: zipfile.ZipFile) -> list[str]:
    try:
        root = ElementTree.fromstring(archive.read("xl/sharedStrings.xml"))
    except KeyError:
        return []
    return ["".join(node.itertext())[:MAX_JSON_STRING] for node in root]


def read_rows(root: Path, relative_path: str) -> list[dict[str, Any]]:
    """Read bounded records from one validated Workspace-relative path."""
    path = _validated_source_path(root, relative_path)
    suffix = path.suffix.lower()
    if suffix == ".csv":
        sample = decode_text(_read_prefix(path, 256 * 1024)).splitlines()[:20]
        try:
            delimiter = csv.Sniffer().sniff("\n".join(sample[:10]), delimiters=",;\t|").delimiter
        except csv.Error:
            delimiter = ","
        with path.open("rb") as binary:
            text_stream = io.TextIOWrapper(
                binary, encoding=detect_encoding(_read_prefix(path, 64 * 1024)), errors="strict"
            )
            rows = csv.DictReader(text_stream, delimiter=delimiter)
            bounded = [
                {str(key).strip(): value for key, value in row.items() if key is not None}
                for row in itertools.islice(rows, MAX_NORMALIZE_ROWS + 1)
            ]
            if len(bounded) > MAX_NORMALIZE_ROWS:
                raise ValueError("source_row_limit_exceeded")
            return bounded
    if suffix == ".json":
        records: list[dict[str, Any]] = []
        with path.open("rb") as stream:
            for prefix in ("item", "rows.item", "orders.item", "records.item", "data.item"):
                stream.seek(0)
                try:
                    for value in ijson.items(stream, prefix):
                        if isinstance(value, dict):
                            records.append(value)
                            if len(records) > MAX_NORMALIZE_ROWS:
                                raise ValueError("source_row_limit_exceeded")
                    if records:
                        break
                except ijson.common.IncompleteJSONError as error:
                    raise ValueError("invalid_json") from error
        return records
    workbook = load_workbook(path, read_only=True, data_only=True, keep_links=False)
    records: list[dict[str, Any]] = []
    try:
        if len(workbook.worksheets) > MAX_XLSX_SHEETS:
            raise ValueError("xlsx_sheet_limit_exceeded")
        for worksheet in workbook.worksheets:
            rows = worksheet.iter_rows(values_only=True)
            header = next(rows, None)
            if not header:
                continue
            if len(header) > MAX_XLSX_COLUMNS:
                raise ValueError("source_column_limit_exceeded")
            columns = [str(value or "").strip() for value in header]
            for values in rows:
                if not any(value not in (None, "") for value in values):
                    continue
                record = {
                    column: values[index] if index < len(values) else ""
                    for index, column in enumerate(columns)
                    if column
                }
                qualified = {
                    f"{worksheet.title}::{column}": value for column, value in record.items()
                }
                records.append({**qualified, **record, "__sheet_name": worksheet.title})
                if len(records) > MAX_NORMALIZE_ROWS:
                    raise ValueError("source_row_limit_exceeded")
    finally:
        workbook.close()
    return records


def read_json_document(root: Path, relative_path: str) -> dict[str, Any]:
    """Read one bounded JSON document from an authorized Workspace source."""
    path = _validated_source_path(root, relative_path)
    if path.suffix.lower() != ".json":
        raise ValueError("workspace_source_must_be_json")
    if path.stat().st_size > 8 * 1024 * 1024:
        raise ValueError("json_source_exceeds_size_limit")
    with path.open("r", encoding=detect_encoding(_read_prefix(path, 64 * 1024))) as stream:
        payload = json.load(stream)
    if not isinstance(payload, dict):
        raise ValueError("json_source_must_be_object")
    return payload


def read_json_document_with_sha256(root: Path, relative_path: str) -> tuple[dict[str, Any], str]:
    """Read one validated JSON source and return its exact byte identity."""
    path = _validated_source_path(root, relative_path)
    if path.suffix.lower() != ".json":
        raise ValueError("workspace_source_must_be_json")
    if path.stat().st_size > 8 * 1024 * 1024:
        raise ValueError("json_source_exceeds_size_limit")
    raw = path.read_bytes()
    try:
        payload = json.loads(raw.decode(detect_encoding(raw[: 64 * 1024])))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError("workspace_json_invalid") from error
    if not isinstance(payload, dict):
        raise ValueError("json_source_must_be_object")
    return payload, hashlib.sha256(raw).hexdigest()


def _read_prefix(path: Path, limit: int) -> bytes:
    with path.open("rb") as stream:
        raw = stream.read(limit + 1)
    if len(raw) > limit:
        return raw[:limit]
    return raw


def detect_encoding(raw: bytes) -> str:
    for encoding in ("utf-8-sig", "utf-16", "utf-16-le", "utf-16-be"):
        try:
            raw.decode(encoding)
            return encoding
        except UnicodeDecodeError:
            continue
    best = from_bytes(raw).best()
    if best is not None and best.encoding and best.percent_chaos < 10:
        return best.encoding
    raise ValueError("unsupported_text_encoding")


def decode_text(raw: bytes) -> str:
    return raw.decode(detect_encoding(raw))


def _iter_json_events(stream: BinaryIO):
    try:
        yield from ijson.parse(stream)
    except (ijson.common.IncompleteJSONError, ijson.common.JSONError) as error:
        raise ValueError("invalid_json") from error


def _json_depth(prefix: str) -> int:
    return sum(1 for token in prefix.split(".") if token and token != "item")
