#!/usr/bin/env python3
"""Decathlon (decathlon.com.tr) purchase history -> ev purchase NDJSON (spec/purchases.md §6).

Reads the raw export an agent saved under ~/.ev/purchases/decathlon/ (outside every
repository; the raw orders carry addresses):

  catalog.json                 one object per line: order_id, date, channel (online|store),
                               order_status, title, brand, item_code, quantity, unit_price,
                               line_total (whole TRY, discounts included), currency, status
                               (purchased|cancelled|"returned (store return transaction)"),
                               seller, product_url, order_detail_url
  invoices.json                per invoice: order_id, parcel, file
  raw/orders/<order>.json      the order detail; parcels[].delivery.status.steps carry the
                               delivery (or store pick-up) date
  raw/invoices/<order>-<parcel>.pdf   the invoices

The history covers online orders and till receipts from a store. A store return is a
transaction of its own (negative total, no product lines in the detail) that names the item
but not the sale it undoes: it marks the latest earlier sale of the same item code that no
other return has taken as `returned`, and emits no line itself. Cancelled lines are dropped.

`delivered_at` is the parcel's "Delivered" step, the store pick-up's "Finished" step, or the
till date for a store sale; orders that only reached "Shipped" have none. Usage:

  tools/purchases/decathlon.py [--dir ~/.ev/purchases/decathlon] | ev buy import --stdin
"""
import argparse
import json
import os
import re
import sys
from pathlib import Path

SOURCE = "decathlon"

# Regexes searched in the lowercased title.
# Balls sold by the pack wear out; a single ball or a trainer ball on a cord is kept.
CONSUMABLE = [r"\bovergrip", r"\btopu - \d+ adet"]
CLOTHING = [
    r"\beldiven", r"\bbilekliği", r"\btişört", r"\bşort\b", r"\btayt\b", r"\bçorap",
    r"\bayakkabı", r"\bmont\b", r"\bceket", r"\bsweatshirt", r"\beşofman",
]
KEEP = [r"\bçanta"]


def tr_lower(s: str) -> str:
    return (s or "").replace("İ", "i").replace("I", "ı").lower()


def bucket(title: str) -> str:
    t = tr_lower(title)
    if any(re.search(p, t) for p in KEEP):
        return "durable"
    if any(re.search(p, t) for p in CLOTHING):
        return "clothing"
    if any(re.search(p, t) for p in CONSUMABLE):
        return "consumable"
    return "durable"


def delivery_date(detail: dict) -> str | None:
    dates = []
    for parcel in detail.get("parcels") or []:
        delivery = parcel.get("delivery") or {}
        method = (delivery.get("shipping") or {}).get("method")
        wanted = {"DKT:Delivered"} | ({"DKT:Finished"} if method == "RetailStore" else set())
        for step in (delivery.get("status") or {}).get("steps") or []:
            if step.get("status") in wanted and step.get("date"):
                dates.append(step["date"][:10])
    return min(dates) if dates else None


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--dir", default="~/.ev/purchases/decathlon")
    args = ap.parse_args()
    root = Path(os.path.expanduser(args.dir))
    catalog = json.loads((root / "catalog.json").read_text())
    invoices_file = root / "invoices.json"
    invoices = json.loads(invoices_file.read_text()) if invoices_file.exists() else []

    sales = [l for l in catalog if l.get("status") == "purchased"]
    returns = [l for l in catalog if str(l.get("status", "")).startswith("returned")]
    returned: set[int] = set()
    for r in sorted(returns, key=lambda l: l["date"]):
        earlier = [
            i for i, s in enumerate(sales)
            if s["item_code"] == r["item_code"] and s["date"] <= r["date"] and i not in returned
        ]
        if earlier:
            returned.add(max(earlier, key=lambda i: sales[i]["date"]))

    keys_by_order: dict[str, list[str]] = {}
    seen: dict[tuple[str, str], int] = {}
    out = sys.stdout
    for i, line in enumerate(sales):
        order, sku = line["order_id"], line["item_code"]
        n = seen.get((order, sku), 0)
        seen[(order, sku)] = n + 1
        key = f"{order}:{sku}" if n == 0 else f"{order}:{sku}:{n + 1}"
        keys_by_order.setdefault(order, []).append(key)
        raw = root / "raw" / "orders" / f"{order}.json"
        detail = json.loads(raw.read_text()) if raw.exists() else {}
        delivered = delivery_date(detail)
        if line.get("channel") == "store":
            delivered = line["date"][:10]
        total = line.get("line_total")
        rec = {
            "type": "purchase",
            "source": SOURCE,
            "key": key,
            "shop": "Decathlon",
            "merchant": line.get("seller") or "Decathlon",
            "order": order,
            "order_url": line.get("order_detail_url"),
            "product_url": line.get("product_url"),
            "sku": sku,
            "name": line.get("title") or line.get("title_at_order"),
            "brand": line.get("brand"),
            "ordered_at": line["date"][:10],
            "delivered_at": delivered,
            "qty": line.get("quantity") or 1,
            "paid": f"{total:.2f}" if total is not None else None,
            "currency": line.get("currency") or "TRY",
            "status": "returned" if i in returned else "delivered",
            "bucket": bucket(line.get("title") or ""),
            "raw": str(raw) if raw.exists() else None,
        }
        out.write(json.dumps({k: v for k, v in rec.items() if v is not None}, ensure_ascii=False))
        out.write("\n")

    for inv in invoices:
        order = inv.get("order_id")
        pdf = root / inv["file"] if inv.get("file") else None
        if order not in keys_by_order or not pdf or not pdf.exists():
            continue
        doc = {
            "type": "document",
            "source": SOURCE,
            "kind": "invoice",
            "file": str(pdf),
            "purchases": keys_by_order[order],
            "issuer": "Decathlon",
        }
        out.write(json.dumps(doc, ensure_ascii=False))
        out.write("\n")


if __name__ == "__main__":
    main()
