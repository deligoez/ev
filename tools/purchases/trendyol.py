#!/usr/bin/env python3
"""Trendyol order history -> ev purchase NDJSON (spec/purchases.md §6).

Reads the raw export an agent saved under ~/.ev/purchases/trendyol/ (outside every repository;
it carries addresses and phone numbers):

  catalog.json                one object per order line of the non-cancelled orders: order_id,
                              date, line_id, shipment, title, brand, content_id, business_unit,
                              quantity, line_total (final price x qty, every discount spread in),
                              seller {store, legal_name}, status, product_url, order_detail_url
  raw/orders/<order>.json     the order detail; per shipment, per line state, the line's leaf
                              category (`productCategory`) and the date it reached that state
  raw/invoices/index.json     per `<order>/<shipment>`: the invoice file, if one could be saved

Writes `purchase` lines, one per order line, and `document` lines for the invoice PDFs, hung on
the lines of their shipment. A shipment whose only record is the seller's notification e-mail
(HTML that links to an e-Archive portal) has no document. The fully cancelled order is not in the
catalog and is not emitted. Usage:

  tools/purchases/trendyol.py [--dir ~/.ev/purchases/trendyol] | ev buy import --stdin
"""
import argparse
import json
import os
import re
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

SOURCE = "trendyol"
# Trendyol timestamps are epoch milliseconds; the dates are read in Turkish time.
TR = timezone(timedelta(hours=3))

# Words that mark a line as used up rather than kept, matched against the lowercased business
# unit, leaf category and title. A kept thing in such a unit (a cat bowl) is listed in KEEP.
CONSUMABLE = [
    r"mama", r"ödül", r"deterjan", r"bulaşık makinesi tablet", r"tablet\b", r"temizlik",
    r"protein tozu", r"takviye", r"vitamin", r"yağı \d+ ?ml", r"şampuan", r"kozmetik", r"parfüm",
    r"gıda", r"kahve", r"\bçay\b",
]
KEEP = [r"\bkap\b", r"kabı", r"oyuncak", r"tasma", r"tırmalama", r"makine", r"cihaz"]
CLOTHING = [
    r"\bwoman\b", r"kadın b\b", r"giyim", r"kazak", r"tayt", r"bluz", r"gömlek", r"pantolon",
    r"t-shirt", r"tişört", r"elbise", r"ayakkabı", r"çorap", r"mont", r"ceket",
]
DIGITAL = [r"dijital", r"e-kitap", r"lisans", r"abonelik"]

STATE = {
    "OUTBOUND_DELIVERED": "delivered",
    "INBOUND_ACCEPTED": "returned",
}


def tr_lower(s: str) -> str:
    return (s or "").replace("İ", "i").replace("I", "ı").lower()


def hits(words: list[str], text: str) -> bool:
    return any(re.search(w, text) for w in words)


def bucket(unit: str, category: str, title: str) -> str:
    t = tr_lower(f"{unit} | {category} | {title}")
    if hits(DIGITAL, t):
        return "digital"
    if hits(CLOTHING, t):
        return "clothing"
    if hits(CONSUMABLE, t):
        # KEEP rescues the machine or bowl itself, never the tablets or detergent made for it.
        kept = hits(KEEP, tr_lower(f"{category} {title}"))
        if not kept or re.search(r"tablet|deterjan|kapsül", t):
            return "consumable"
    return "durable"


def shipment_lines(order_file: Path) -> dict[str, tuple[str, dict]]:
    """Line id -> (state, line record) from the order detail's shipments."""
    out: dict[str, tuple[str, dict]] = {}
    if not order_file.exists():
        return out
    order = json.loads(order_file.read_text()).get("order") or {}
    for shipment in (order.get("shipments") or {}).values():
        for state, items in (shipment.get("products") or {}).items():
            for item in items:
                out[str(item.get("id"))] = (state, item)
    return out


def day(ms) -> str | None:
    if not ms:
        return None
    return datetime.fromtimestamp(int(ms) / 1000, TR).strftime("%Y-%m-%d")


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--dir", default="~/.ev/purchases/trendyol")
    args = ap.parse_args()
    root = Path(os.path.expanduser(args.dir))
    catalog = json.loads((root / "catalog.json").read_text())

    keys_by_shipment: dict[tuple[str, str], list[str]] = {}
    sellers_by_shipment: dict[tuple[str, str], set[str]] = {}
    details: dict[str, dict[str, tuple[str, dict]]] = {}
    out = sys.stdout
    seen: set[str] = set()
    for line in catalog:
        order = str(line["order_id"])
        # The line id is Trendyol's own id for the order line, unique across the account.
        key = f"{order}:{line['line_id']}"
        if key in seen:
            raise SystemExit(f"duplicate key {key}")
        seen.add(key)
        raw = root / "raw" / "orders" / f"{order}.json"
        if order not in details:
            details[order] = shipment_lines(raw)
        state, item = details[order].get(str(line["line_id"]), (line.get("status") or "", {}))
        status = STATE.get(state, "returned" if "return" in (line.get("status_label") or "") else "delivered")
        category = item.get("productCategory")
        seller = (line.get("seller") or {}).get("store")
        shipment = str(line.get("shipment") or "")
        keys_by_shipment.setdefault((order, shipment), []).append(key)
        if seller:
            sellers_by_shipment.setdefault((order, shipment), set()).add(seller)
        rec = {
            "type": "purchase",
            "source": SOURCE,
            "key": key,
            "shop": "Trendyol",
            "merchant": seller,
            "order": order,
            "order_url": line.get("order_detail_url"),
            "product_url": line.get("product_url"),
            "sku": str(line["content_id"]) if line.get("content_id") else None,
            "name": line["title"],
            "brand": line.get("brand"),
            "category": category or line.get("business_unit"),
            "ordered_at": (line.get("date") or "")[:10] or None,
            # The date a line reached its state: delivery for a delivered line. A returned line's
            # date is the return's, so it carries none.
            "delivered_at": day(item.get("date")) if state == "OUTBOUND_DELIVERED" else None,
            "qty": line.get("quantity") or 1,
            "paid": f"{line['line_total']:.2f}" if line.get("line_total") is not None else None,
            "currency": ((item.get("product") or {}).get("price") or {}).get("currency") or "TRY",
            "status": status,
            "bucket": bucket(line.get("business_unit") or "", category or "", line["title"]),
            "raw": str(raw) if raw.exists() else None,
        }
        out.write(json.dumps({k: v for k, v in rec.items() if v is not None}, ensure_ascii=False))
        out.write("\n")

    index_file = root / "raw" / "invoices" / "index.json"
    index = json.loads(index_file.read_text()) if index_file.exists() else {}
    for ref, inv in sorted(index.items()):
        order, _, shipment = ref.partition("/")
        keys = keys_by_shipment.get((order, shipment))
        if not keys:
            continue
        # The saved file itself, or the PDF a viewer page or wrapper led to.
        files = [inv.get("file")] + [f.get("file") for f in inv.get("followed") or []]
        pdf = next(
            (root / f for f in files if f and f.endswith(".pdf") and (root / f).exists()), None
        )
        if pdf is None:
            continue
        ms = sellers_by_shipment.get((order, shipment), set())
        doc = {
            "type": "document",
            "source": SOURCE,
            "kind": "invoice",
            "file": str(pdf),
            "purchases": keys,
            "issuer": next(iter(ms)) if len(ms) == 1 else None,
        }
        out.write(json.dumps({k: v for k, v in doc.items() if v is not None}, ensure_ascii=False))
        out.write("\n")


if __name__ == "__main__":
    main()
