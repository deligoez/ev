#!/usr/bin/env python3
"""n11 order history -> ev purchase NDJSON (spec/purchases.md §6).

Reads the raw export an agent saved under ~/.ev/purchases/n11/ (outside every repository; it
carries addresses and phone numbers):

  catalog.json                 one object per order line: order_id, date, order_item_id,
                               product_title, product_id, sku_id, quantity, line_total (after
                               store discounts), currency, seller, line_status, product_url,
                               order_detail_url
  raw/orders/<order>.json      the order detail (`window.model.orderDetails`); per shipment the
                               delivery date ("Teslim tarihi") and the progress bar
  raw/products/index.json      the product pages fetched later; live ones carry brand and category
  raw/invoices/index.json      per seller group: the downloaded invoice file, if any

The catalog carries no category, and live product pages are often gone or catalog-unified, so
the bucket is decided from the line's title (and the live category when there is one). The line
amount is the line's own total; order-level shipping, instalment charges and basket coupons are
not spread over lines. Writes `purchase` lines, one per order line, and `document` lines for the
seller invoices, hung on that seller's lines of the order. Usage:

  tools/purchases/n11.py [--dir ~/.ev/purchases/n11] | ev buy import --stdin
"""
import argparse
import json
import os
import re
import sys
from pathlib import Path

SOURCE = "n11"

# Words that mark a line as used up rather than kept, matched as regexes against the lowercased
# title and category. A kept object whose title also hits one (a tea machine, a vacuum cleaner
# without a bag) is rescued by KEEP.
CONSUMABLE = [
    r"deterjan", r"bulaşık (makinesi )?(tablet|kapsül)", r"kapsülü", r"temizlik ürünü",
    r"\bmama", r"yulaf", r"müsli", r"\bun[u]?\b", r"yemişi", r"zeytinyağı", r"\bsabun",
    r"yağı \d+ ?ml", r"eldiven", r"filtresi", r"toz torbası\b", r"nem boyası", r"yapıştırıcı",
    r"buğu önleyici", r"\bdiş\b", r"\bedt\b", r"parfüm", r"şampuan",
]
KEEP = [r"makine", r"sebil", r"süpürge"]
CLOTHING = [r"\bmont\b", r"\bceket\b", r"\bpolar\b", r"\btrk\b", r"eşofman", r"tişört", r"gömlek"]
DIGITAL = [r"lisans", r"e-kitap", r"abonelik"]

MONTHS = {
    "ocak": 1, "şubat": 2, "mart": 3, "nisan": 4, "mayıs": 5, "haziran": 6, "temmuz": 7,
    "ağustos": 8, "eylül": 9, "ekim": 10, "kasım": 11, "aralık": 12,
}
STATUS = {"delivered/completed": "delivered", "returned": "returned", "cancelled (claim)": "cancelled"}


def tr_lower(s: str) -> str:
    return (s or "").replace("İ", "i").replace("I", "ı").lower()


def hits(words: list[str], text: str) -> bool:
    return any(re.search(w, text) for w in words)


def bucket(title: str, category: str) -> str:
    t = tr_lower(f"{title} {category}")
    if hits(DIGITAL, t):
        return "digital"
    if hits(CLOTHING, t):
        return "clothing"
    if hits(CONSUMABLE, t):
        # A consumable word wins unless the line is clearly the machine itself.
        if hits(KEEP, t) and not re.search(r"deterjan|tablet|kapsül|toz torbası\b", t):
            return "durable"
        return "consumable"
    return "durable"


def tr_date(text: str) -> str | None:
    """'18 Şubat 2025' -> '2025-02-18'."""
    m = re.match(r"\s*(\d{1,2})\s+(\S+)\s+(\d{4})", text or "")
    if not m or tr_lower(m.group(2)) not in MONTHS:
        return None
    return f"{int(m.group(3)):04d}-{MONTHS[tr_lower(m.group(2))]:02d}-{int(m.group(1)):02d}"


def delivered_dates(order_file: Path) -> dict[int, str]:
    """Order item id -> delivery date, for shipments whose progress reached the last step."""
    out: dict[int, str] = {}
    if not order_file.exists():
        return out
    order = json.loads(order_file.read_text()).get("order") or {}
    for sg in order.get("orderItemSellerGroups") or []:
        for g in sg.get("orderItemGroups") or []:
            gi = g.get("generalInformation") or {}
            bar = g.get("progressBar") or {}
            steps = bar.get("steps") or []
            done = steps and bar.get("currentLocation") == len(steps) - 1
            date = tr_date(gi.get("deliveryTimeInfoDate") or "")
            if not (done and date and (gi.get("deliveryTimeInfoText") or "").startswith("Teslim")):
                continue
            for item in g.get("orderItemList") or []:
                out[item.get("id")] = date
    return out


def live_product(root: Path, entry: dict) -> tuple[str | None, str | None]:
    """Brand and deepest category from a still-live product page, when it names them."""
    if not entry.get("file") or entry.get("http") != 200 or "removed" in (entry.get("availability") or ""):
        return None, None
    page = root / "raw" / "products" / entry["file"]
    if not page.exists():
        return None, None
    html = page.read_text(errors="replace")
    brand = re.search(r'"brandName":"([^"]+)"', html)
    cats = re.search(r'"categoryAndParentList":\[([^\]]*)\]', html)
    category = None
    if cats:
        parts = re.findall(r'"\d+\|([^"]+)"', cats.group(1))
        category = " > ".join(parts) or None
    return (brand.group(1) if brand else None), category


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--dir", default="~/.ev/purchases/n11")
    args = ap.parse_args()
    root = Path(os.path.expanduser(args.dir))
    catalog = json.loads((root / "catalog.json").read_text())
    products_index = root / "raw" / "products" / "index.json"
    products = (
        {str(p["product_id"]): p for p in json.loads(products_index.read_text())}
        if products_index.exists()
        else {}
    )

    keys_by_seller: dict[tuple[str, str], list[str]] = {}
    delivered_cache: dict[str, dict[int, str]] = {}
    out = sys.stdout
    seen: set[str] = set()
    for line in catalog:
        order = str(line["order_id"])
        # The order item id is n11's own id for the line, unique across the account.
        key = f"{order}:{line['order_item_id']}"
        if key in seen:
            raise SystemExit(f"duplicate key {key}")
        seen.add(key)
        raw = root / "raw" / "orders" / f"{order}.json"
        if order not in delivered_cache:
            delivered_cache[order] = delivered_dates(raw)
        status = STATUS.get(line.get("line_status") or "", "delivered")
        brand, category = live_product(root, products.get(str(line.get("product_id")), {}))
        seller = line.get("seller")
        keys_by_seller.setdefault((order, seller or ""), []).append(key)
        rec = {
            "type": "purchase",
            "source": SOURCE,
            "key": key,
            "shop": "n11",
            "merchant": seller,
            "order": order,
            "order_url": line.get("order_detail_url"),
            "product_url": line.get("product_url"),
            "sku": str(line["product_id"]) if line.get("product_id") is not None else None,
            "name": line["product_title"],
            "brand": brand,
            "category": category,
            "ordered_at": (line.get("date") or "")[:10] or None,
            "delivered_at": (
                delivered_cache[order].get(line.get("order_item_id")) if status != "cancelled" else None
            ),
            "qty": line.get("quantity") or 1,
            "paid": f"{line['line_total']:.2f}" if line.get("line_total") is not None else None,
            "currency": line.get("currency") or "TRY",
            "status": status,
            "bucket": bucket(line["product_title"], category or ""),
            "raw": str(raw) if raw.exists() else None,
        }
        out.write(json.dumps({k: v for k, v in rec.items() if v is not None}, ensure_ascii=False))
        out.write("\n")

    invoices_index = root / "raw" / "invoices" / "index.json"
    invoices = json.loads(invoices_index.read_text()) if invoices_index.exists() else []
    for inv in invoices:
        if inv.get("status") != "downloaded" or not inv.get("file"):
            continue
        pdf = root / "raw" / "invoices" / inv["file"]
        keys = keys_by_seller.get((str(inv["order_id"]), inv.get("seller") or ""))
        if not keys or not pdf.exists():
            continue
        doc = {
            "type": "document",
            "source": SOURCE,
            "kind": "invoice",
            "file": str(pdf),
            "purchases": keys,
            "issuer": inv.get("seller"),
        }
        out.write(json.dumps({k: v for k, v in doc.items() if v is not None}, ensure_ascii=False))
        out.write("\n")


if __name__ == "__main__":
    main()
