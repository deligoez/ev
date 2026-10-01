#!/usr/bin/env python3
"""Hepsiburada order history -> ev purchase NDJSON (spec/purchases.md §6).

Reads the raw export an agent saved under ~/.ev/purchases/hepsiburada/ (outside every
repository; it carries addresses and phone numbers):

  catalog.json          one object per line: order, date, sku, name, brand, category, qty,
                        paid, merchant, returned, digital, url
  orders-detail.json    per order: created and, per item, status, received, currency
  raw/invoices/<order>-<n>.pdf   the e-Archive invoices
  raw/orders/<order>.json        the raw order record a line points at

Writes `purchase` lines, one per bought line, and `document` lines for the invoices, hung on
every line of their order. Consumables are classified and dropped (spec §11), cancelled lines
too. Usage:

  tools/purchases/hepsiburada.py [--dir ~/.ev/purchases/hepsiburada] | ev buy import --stdin
"""
import argparse
import json
import os
import sys
from pathlib import Path

SOURCE = "hepsiburada"

# Category words that mark a line as used up rather than kept. Matched against the lowercased
# category name; a kept thing in a consumable-sounding category (a cat bowl) is listed in KEEP.
CONSUMABLE = [
    "mama", "kedi kumu", "deterjan", "temizlik", "besin", "yağ", "kahve", "çay", "içecek", "mendil",
    "atıştırmalık", "ezme", "unlu mamül", "aktar", "maske", "medikal", "kozmetik", "şampuan",
    "makyaj", "parfüm", "gübre", "haşere", "kartuş", "ödüller ve vitamin", "bakım ürün",
]
KEEP = [
    "kap", "tuvalet", "tırmalama", "tasma", "oyuncak", "seyahat", "mama ve su", "makina",
    "makine",
]
CLOTHING = ["pijama", "tshirt", "tayt", "gömlek", "giyim", "ayakkabı", "çorap", "mont", "kışlık"]
DIGITAL = ["online lisans", "premium"]

# ISO 4217 numeric codes the export uses.
CURRENCY = {"949": "TRY", "978": "EUR", "840": "USD"}


def bucket(category: str, digital: bool) -> str:
    c = (category or "").lower()
    if digital or any(w in c for w in DIGITAL):
        return "digital"
    if any(w in c for w in CLOTHING):
        return "clothing"
    if any(w in c for w in CONSUMABLE) and not any(w in c for w in KEEP):
        return "consumable"
    return "durable"


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--dir", default="~/.ev/purchases/hepsiburada")
    args = ap.parse_args()
    root = Path(os.path.expanduser(args.dir))
    catalog = json.loads((root / "catalog.json").read_text())
    detail = {o["order"]: o for o in json.loads((root / "orders-detail.json").read_text())}

    keys_by_order: dict[str, list[str]] = {}
    merchants: dict[str, set[str]] = {}
    out = sys.stdout
    seen: dict[tuple[str, str], int] = {}
    for line in catalog:
        order, sku = line["order"], line["sku"]
        # The same product twice in one order (one unit kept, one returned) is two lines: the
        # n-th of them is the n-th item with that SKU.
        n = seen.get((order, sku), 0)
        seen[(order, sku)] = n + 1
        items = [i for i in detail.get(order, {}).get("items", []) if i["sku"] == sku]
        item = items[n] if n < len(items) else {}
        status = {"Delivered": "delivered", "Returned": "returned", "Cancelled": "cancelled"}.get(
            item.get("status", ""), "returned" if line.get("returned") else "delivered"
        )
        key = f"{order}:{sku}" if n == 0 else f"{order}:{sku}:{n + 1}"
        keys_by_order.setdefault(order, []).append(key)
        if line.get("merchant"):
            merchants.setdefault(order, set()).add(line["merchant"])
        raw = root / "raw" / "orders" / f"{order}.json"
        rec = {
            "type": "purchase",
            "source": SOURCE,
            "key": key,
            "shop": "Hepsiburada",
            "merchant": line.get("merchant"),
            "order": order,
            "product_url": line.get("url"),
            "sku": sku,
            "name": line["name"],
            "brand": line.get("brand"),
            "category": line.get("category"),
            "ordered_at": (detail.get(order, {}).get("created") or line.get("date") or "")[:10]
            or None,
            "delivered_at": (item.get("received") or "")[:10] or None,
            "qty": line.get("qty") or 1,
            "paid": f"{line['paid']:.2f}" if line.get("paid") is not None else None,
            "currency": CURRENCY.get(str(item.get("currency")), "TRY"),
            "status": status,
            "bucket": bucket(line.get("category", ""), line.get("digital", False)),
            "raw": str(raw) if raw.exists() else None,
        }
        out.write(json.dumps({k: v for k, v in rec.items() if v is not None}, ensure_ascii=False))
        out.write("\n")

    for pdf in sorted((root / "raw" / "invoices").glob("*.pdf")):
        order = pdf.stem.split("-")[0]
        if order not in keys_by_order:
            continue
        ms = merchants.get(order, set())
        doc = {
            "type": "document",
            "source": SOURCE,
            "kind": "invoice",
            "file": str(pdf),
            "purchases": keys_by_order[order],
            "issuer": next(iter(ms)) if len(ms) == 1 else None,
        }
        out.write(json.dumps({k: v for k, v in doc.items() if v is not None}, ensure_ascii=False))
        out.write("\n")


if __name__ == "__main__":
    main()
