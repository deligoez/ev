#!/usr/bin/env python3
"""Amazon.com.tr order history -> ev purchase NDJSON (spec/purchases.md §6).

Reads the raw export an agent saved under ~/.ev/purchases/amazon-com-tr/ (outside every
repository; the raw pages carry addresses):

  catalog.json               one object per order line: order_id, date, title, asin, product_url,
                             quantity, unit_price (VAT included), line_total, seller,
                             shipment_status, order_status, digital, detail_url
  raw/invoices/index.json    per order: the documents its invoice popover offered (label, file)
  raw/invoices/<order>-<n>.pdf|xml   invoices ("Fatura", "Garanti / Pslip") and credit notes
  raw/orders/<order>.html    the raw order page a line points at

Fully cancelled orders were never fetched, so every catalog line was bought. `order_status` is
per order: in a returned order only the lines that carry a shipment status of their own were
returned (the refund total matches them), the others were kept. The site shows a delivery date
("Teslim edildi: 3 Temmuz") without a year on a few recent lines only; the year comes from the
order date.

`paid` is the line total (unit price x quantity, VAT included). Order-level promotions,
shipping and import deposits are not spread over the lines. Titles carry no category, so the
bucket is guessed from words in the title; it is editable in ev.

Writes `purchase` lines, one per order line, and `document` lines for the invoice PDFs, hung
on every line of their order (credit notes and the UBL XML twins are left out). Consumable
lines are emitted with their bucket; ev skips them (spec §11). Usage:

  tools/purchases/amazon-com-tr.py [--dir ~/.ev/purchases/amazon-com-tr] | ev buy import --stdin
"""
import argparse
import html
import json
import os
import re
import sys
from pathlib import Path

SOURCE = "amazon-com-tr"

# Regexes that mark a line as used up rather than kept, searched in the lowercased title. Each
# starts at a word boundary, so a short word does not fire inside a longer one ("mont" in
# "montaj"). STRONG wins over KEEP (a dishwasher detergent names the machine it is for).
STRONG = [
    r"deterjan", r"\bmaması", r"\bkedi yaş mama", r"\bkedi kumu", r"\bçözücü tablet",
    r"(?<!erkek )\bbakım seti", r"\byedek parça",
]
CONSUMABLE = [
    r"\bmakarna", r"\bşehriye", r"\bpirinç", r"\bbulgur", r"\bmercimek", r"\bfasulye",
    r"\bnohut", r"\bsalça", r"\breçel", r"\bşeker\b", r"\bmayası", r"\bkek\b", r"\bchia\b",
    r"\bprotein\b", r"\bdevam sütü", r"\biçecek", r"\bçay", r"\bkahve", r"\bşurup", r"\btuzu\b",
    r"\bayçiçek", r"\byağ çözücü", r"\bçamaşır suyu", r"\bmendil", r"\bsabun(u|lar)?\b",
    r"\bmaske\b", r"\bkremi\b", r"\bdiş ipi", r"\bburun bandı", r"\bvitamin", r"\btakviye",
    r"\bkartuş", r"\brefil", r"\byedek\b", r"\bfiltre(si|leri)?\b", r"\bçözücü tablet",
    r"\bpoşet", r"\bbakım seti", r"\bmastik", r"\bdolgu", r"\bçamur bant", r"\byapıştırıcı",
    r"\bambalaj kağıdı", r"\bpaketleme kağıdı", r"\btransfer şerit", r"\bplastik şerit",
    r"\betiket muadili", r"\bparfüm", r"\bunu\b", r"\bpiller", r"\b(kalem|düğme|lityum|lithium|alkalin|oyuncak) pil",
]
KEEP = [
    r"\boyuncak", r"\blaboratuvar", r"\bmakinesi", r"\bmakinası", r"\bkartuşu dahil",
    r"\berkek bakım", r"\bkupa", r"\bbardağı", r"\bkaraf", r"\bmatara", r"\bstandı",
    r"\btutucu", r"\bkılıf", r"\borganizer", r"\bfırça",
]
CLOTHING = [
    r"\bçorap", r"\bceket", r"\bşort\b", r"\btişört", r"\bt-shirt", r"\bpantolon",
    r"\bayakkabı", r"\bmont\b", r"\bgömlek", r"\belbise", r"\bkazak\b", r"\btayt\b",
    r"\beldiven", r"\bpijama", r"\bpatik\b",
]
DIGITAL = [r"\be-kitap sürümü", r"\bkindle sürümü", r"\babonelik", r"\bprime video"]

MONTHS = {
    "ocak": 1, "şubat": 2, "mart": 3, "nisan": 4, "mayıs": 5, "haziran": 6, "temmuz": 7,
    "ağustos": 8, "eylül": 9, "ekim": 10, "kasım": 11, "aralık": 12,
}


def tr_lower(s: str) -> str:
    return (s or "").replace("İ", "i").replace("I", "ı").lower()


def hit(text: str, patterns: list[str]) -> bool:
    return any(re.search(p, text) for p in patterns)


def bucket(title: str, digital: bool) -> str:
    t = tr_lower(title)
    if digital or hit(t, DIGITAL):
        return "digital"
    if hit(t, CLOTHING):
        return "clothing"
    if hit(t, STRONG) or (hit(t, CONSUMABLE) and not hit(t, KEEP)):
        return "consumable"
    return "durable"


def delivered(text: str | None, ordered: str) -> str | None:
    """'Teslim edildi: 03 Temmuz ...' -> a date, the year taken from the order date."""
    m = re.search(r"Teslim edildi:\s*(\d{1,2})\s+(\w+)", text or "")
    if not m or tr_lower(m.group(2)) not in MONTHS:
        return None
    day, month = int(m.group(1)), MONTHS[tr_lower(m.group(2))]
    year = int(ordered[:4])
    if month < int(ordered[5:7]):
        year += 1
    return f"{year:04d}-{month:02d}-{day:02d}"


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--dir", default="~/.ev/purchases/amazon-com-tr")
    args = ap.parse_args()
    root = Path(os.path.expanduser(args.dir))
    catalog = json.loads((root / "catalog.json").read_text())
    index_file = root / "raw" / "invoices" / "index.json"
    invoices = json.loads(index_file.read_text()) if index_file.exists() else []

    keys_by_order: dict[str, list[str]] = {}
    merchants: dict[str, set[str]] = {}
    seen: dict[tuple[str, str], int] = {}
    out = sys.stdout
    for line in catalog:
        order, sku = line["order_id"], line["asin"]
        n = seen.get((order, sku), 0)
        seen[(order, sku)] = n + 1
        key = f"{order}:{sku}" if n == 0 else f"{order}:{sku}:{n + 1}"
        keys_by_order.setdefault(order, []).append(key)
        if line.get("seller"):
            merchants.setdefault(order, set()).add(line["seller"])
        shipment = line.get("shipment_status") or ""
        returned = line.get("order_status") == "returned" and "iade" in tr_lower(shipment)
        title = html.unescape(line["title"])
        raw = root / "raw" / "orders" / f"{order}.html"
        total = line.get("line_total")
        rec = {
            "type": "purchase",
            "source": SOURCE,
            "key": key,
            "shop": "Amazon.com.tr",
            "merchant": line.get("seller"),
            "order": order,
            "order_url": line.get("detail_url"),
            "product_url": line.get("product_url"),
            "sku": sku,
            "name": title,
            "ordered_at": line.get("date"),
            "delivered_at": delivered(shipment, line["date"]) if line.get("date") else None,
            "qty": line.get("quantity") or 1,
            "paid": f"{total:.2f}" if total is not None else None,
            "currency": "TRY",
            "status": "returned" if returned else "delivered",
            "bucket": bucket(title, line.get("digital", False)),
            "raw": str(raw) if raw.exists() else None,
        }
        out.write(json.dumps({k: v for k, v in rec.items() if v is not None}, ensure_ascii=False))
        out.write("\n")

    for entry in invoices:
        order = entry.get("order_id")
        if order not in keys_by_order:
            continue
        ms = merchants.get(order, set())
        for d in entry.get("docs", []):
            label = tr_lower(d.get("label", ""))
            if d.get("ext") != "pdf" or not ("fatura" in label or "pslip" in label):
                continue
            pdf = root / d["file"]
            if not pdf.exists():
                continue
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
