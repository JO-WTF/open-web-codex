import csv
from datetime import date
from io import TextIOWrapper

from workspace_data import load_dataset_release


REQUIRED_COLUMNS = {
    "shipment_id",
    "province",
    "promised_date",
    "delivered_date",
    "units",
}


def audit_delivery_commitments(arguments):
    release = load_dataset_release(
        workspace_id=arguments["workspace_id"],
        release_id=arguments["release_id"],
        dataset_id=arguments["dataset_id"],
        version=arguments["version"],
        content_sha256=arguments["content_sha256"],
    )
    source = release.require_file("deliveries.csv")
    rows = []
    seen_shipments = set()
    with source.open_binary() as raw:
        reader = csv.DictReader(TextIOWrapper(raw, encoding="utf-8", newline=""))
        if set(reader.fieldnames or []) != REQUIRED_COLUMNS:
            raise ValueError("deliveries.csv columns do not match the tutorial contract")
        for row_number, row in enumerate(reader, start=2):
            shipment_id = row["shipment_id"].strip()
            province = row["province"].strip()
            if not shipment_id or shipment_id in seen_shipments:
                raise ValueError(f"Shipment ID is missing or duplicated at row {row_number}")
            if not province:
                raise ValueError(f"Province is missing at row {row_number}")
            try:
                promised = date.fromisoformat(row["promised_date"])
                delivered = date.fromisoformat(row["delivered_date"])
                units = int(row["units"])
            except (TypeError, ValueError) as error:
                raise ValueError(f"Invalid date or units at row {row_number}") from error
            if units <= 0:
                raise ValueError(f"Units must be positive at row {row_number}")
            seen_shipments.add(shipment_id)
            rows.append(
                {
                    "shipment_id": shipment_id,
                    "province": province,
                    "units": units,
                    "late_days": max(0, (delivered - promised).days),
                }
            )

    if not rows:
        raise ValueError("deliveries.csv contains no shipment rows")

    total_units = sum(row["units"] for row in rows)
    on_time_rows = [row for row in rows if row["late_days"] == 0]
    on_time_units = sum(row["units"] for row in on_time_rows)
    province_totals = {}
    for row in rows:
        province = province_totals.setdefault(
            row["province"],
            {"shipments": 0, "units": 0, "on_time_shipments": 0, "on_time_units": 0},
        )
        province["shipments"] += 1
        province["units"] += row["units"]
        if row["late_days"] == 0:
            province["on_time_shipments"] += 1
            province["on_time_units"] += row["units"]

    provinces = []
    for province_name, totals in province_totals.items():
        provinces.append(
            {
                "province": province_name,
                **totals,
                "on_time_rate": round(
                    totals["on_time_shipments"] / totals["shipments"],
                    6,
                ),
                "demand_weighted_on_time_rate": round(
                    totals["on_time_units"] / totals["units"],
                    6,
                ),
            }
        )
    provinces.sort(key=lambda item: (item["on_time_rate"], item["province"]))

    late_shipments = [
        {
            "shipment_id": row["shipment_id"],
            "province": row["province"],
            "late_days": row["late_days"],
        }
        for row in rows
        if row["late_days"] > 0
    ]
    late_shipments.sort(key=lambda item: (-item["late_days"], item["shipment_id"]))

    return {
        "schema_version": "delivery_audit_report.v1",
        "source": {
            "workspace_id": release.workspace_id,
            "release_id": release.release_id,
            "dataset_id": release.dataset_id,
            "version": release.version,
            "content_sha256": release.content_sha256,
            "logical_file": source.logical_name,
            "file_content_sha256": source.content_sha256,
        },
        "totals": {
            "shipments": len(rows),
            "units": total_units,
            "on_time_shipments": len(on_time_rows),
            "late_shipments": len(rows) - len(on_time_rows),
            "on_time_rate": round(len(on_time_rows) / len(rows), 6),
            "demand_weighted_on_time_rate": round(on_time_units / total_units, 6),
        },
        "worst_province": provinces[0]["province"],
        "provinces": provinces,
        "late_shipments": late_shipments,
        "checks": [
            "The exact Dataset Release identity and every file hash were verified.",
            "Shipment IDs are non-empty and unique.",
            "Dates are ISO dates and units are positive integers.",
            "On-time means delivered_date is not later than promised_date.",
        ],
    }
