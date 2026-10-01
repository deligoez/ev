#!/usr/bin/env python3
"""Robo90 order history -> ev purchase NDJSON (spec/purchases.md §6).

Reads the raw export an agent saved under ~/.ev/purchases/robo90/ (outside every repository; it
carries addresses and phone numbers):

  raw/api/order-list.json   the account's order list as the shop's API returned it: per order
                            ORDER_NUMBER, DATE ('DD.MM.YYYY<br />HH:MM'), STATUS_TEXT,
                            SHIPMENTS[] (ID, DELIVERY_DATE as epoch seconds) and PRODUCT_LIST[]
                            (PRODUCT_ID, PRODUCT_CODE, TITLE, COUNT, COUNT_REFUND, PRICE_TOTAL =
                            line total incl. VAT after discounts, CURRENCY, SHIPMENT_ID, URL)
  catalog.json              the same lines flattened, with the saved product page per line
  raw/products/<id>.html    the product page; its JSON-LD names the brand and category path

PRICE_TOTAL sums to the order total for every order (cargo was free). No invoice was saved, so
no document lines are written. 3D printer filament is used up and classified consumable; tools,
nozzles, cables and components are durable. Usage:

  tools/purchases/robo90.py [--dir ~/.ev/purchases/robo90] | ev buy import --stdin
"""
import argparse
import json
import os
import re
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

SOURCE = "robo90"
SITE = "https://www.robo90.com/"
TR = timezone(timedelta(hours=3))

# Used-up supplies, matched against the lowercased title.
CONSUMABLE = [r"\bfilament\b", r"lehim teli", r"\byağ\b", r"temizleyici"]


def tr_lower(s: str) -> str:
    return (s or "").replace("İ", "i").replace("I", "ı").lower()


def bucket(title: str) -> str:
    t = tr_lower(title)
    # A filament accessory (a vacuum pump, a dryer box) is kept; the filament spool is used up.
    if re.search(r"pompa|kutu|kurutucu|tutucu|askı", t):
        return "durable"
    if any(re.search(w, t) for w in CONSUMABLE):
        return "consumable"
    return "durable"


def page_meta(page: Path) -> tuple[str | None, str | None]:
    """Brand and category path from the product page's JSON-LD."""
    if not page.exists():
        return None, None
    html = page.read_text(errors="replace")
    brand = re.search(r'"brand":\{"@type":"Brand","name":"((?:[^"\\]|\\.)*)"', html)
    category = re.search(r'"category":"((?:[^"\\]|\\.)*)"', html)
    decode = lambda m: json.loads(f'"{m.group(1)}"') if m else None  # noqa: E731
    return decode(brand), decode(category)


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--dir", default="~/.ev/purchases/robo90")
    args = ap.parse_args()
    root = Path(os.path.expanduser(args.dir))
    api = root / "raw" / "api" / "order-list.json"
    orders = json.loads(api.read_text())["ORDERS"]

    out = sys.stdout
    seen: dict[tuple[str, str], int] = {}
    for o in orders:
        order = o["ORDER_NUMBER"]
        m = re.match(r"(\d{2})\.(\d{2})\.(\d{4})", o.get("DATE") or "")
        ordered = f"{m.group(3)}-{m.group(2)}-{m.group(1)}" if m else None
        delivered = {}
        for s in o.get("SHIPMENTS") or []:
            ts = str(s.get("DELIVERY_DATE") or "")
            if ts.isdigit() and int(ts) > 0:
                delivered[str(s.get("ID"))] = datetime.fromtimestamp(int(ts), TR).strftime("%Y-%m-%d")
        order_done = (o.get("STATUS_TEXT") or "") == "Teslim Edildi"
        for p in o.get("PRODUCT_LIST") or []:
            sku = p.get("PRODUCT_CODE") or p["PRODUCT_ID"]
            n = seen.get((order, sku), 0)
            seen[(order, sku)] = n + 1
            key = f"{order}:{sku}" if n == 0 else f"{order}:{sku}:{n + 1}"
            qty = int(p.get("COUNT") or 1)
            refunded = int(p.get("COUNT_REFUND") or 0)
            if refunded >= qty:
                status = "returned"
            elif "iptal" in tr_lower(o.get("STATUS_TEXT") or ""):
                status = "cancelled"
            else:
                status = "delivered"
            brand, category = page_meta(root / "raw" / "products" / f"{p['PRODUCT_ID']}.html")
            rec = {
                "type": "purchase",
                "source": SOURCE,
                "key": key,
                "shop": "Robo90",
                "order": order,
                "product_url": SITE + p["URL"] if p.get("URL") else None,
                "sku": sku,
                "name": p["TITLE"],
                "brand": brand,
                "category": category,
                "ordered_at": ordered,
                "delivered_at": delivered.get(str(p.get("SHIPMENT_ID"))) if order_done else None,
                "qty": qty,
                "paid": f"{float(p['PRICE_TOTAL']):.2f}" if p.get("PRICE_TOTAL") is not None else None,
                "currency": {"TL": "TRY"}.get(p.get("CURRENCY") or "TL", p.get("CURRENCY")),
                "status": status,
                "bucket": bucket(p["TITLE"]),
                "raw": str(api),
            }
            out.write(json.dumps({k: v for k, v in rec.items() if v is not None}, ensure_ascii=False))
            out.write("\n")


if __name__ == "__main__":
    main()
