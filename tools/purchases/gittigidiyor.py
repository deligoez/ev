#!/usr/bin/env python3
"""GittiGidiyor purchases, rebuilt from the shop's e-mails -> ev purchase NDJSON (spec §6).

The site closed in 2022, so the only record is the mail it sent. An agent copied those mails
under ~/.ev/purchases/gittigidiyor/ (outside every repository; they carry addresses):

  messages.json        per mail: rowid, file (raw/mail/<rowid>.emlx), subject, date_received,
                       kind (paid, shipped, confirm_request, auto_confirmed, return_started,
                       cargo_cancelled, seller_message, account)
  raw/mail/*.emlx      the mails themselves (Apple Mail's emlx: a byte count, then the message)

catalog.json in the same folder groups the mails by listing id only. That merges two separate
purchases of the same listing and reads a "2 Adet ..." listing title as a quantity, so this
adapter goes back to the mails. A purchase is one item block of a payment mail: the listing id
"(# <id>)", the price, the purchase time and, in later years, a sale code. Shipping, reminder
and auto-confirm mails are attached to it by sale code, else by purchase time, else by listing
id when the listing was bought once; they add the seller. A started return marks it returned.
A "cargo cancelled" mail is about the tracking number, not the sale, and changes nothing.

Keys are `<listing id>:<purchase time YYYYMMDDHHMM>`, stable and unique per sale. The price
in the mail is what the buyer paid for the line (after any discount the mail lists); the mail
has no delivery date. No invoices exist in the export. Usage:

  tools/purchases/gittigidiyor.py [--dir ~/.ev/purchases/gittigidiyor] | ev buy import --stdin
"""
import argparse
import datetime as dt
import email
import json
import os
import re
import sys
from email import policy
from html import unescape
from pathlib import Path

SOURCE = "gittigidiyor"

PRICE = ("ürünü satın aldığınız fiyat", "ürünün fiyatı", "kapanış fiyatı", "fiyat",
         "ürünün satıldığı fiyat")
WHEN = ("satın alma tarihi",)
CODE = ("satış kodu", "ürün satış kodu")
SELLER = ("satıcı",)

CONSUMABLE = ["alçı", "scotchcast", "bandaj", "deterjan", "kartuş", "toner", "mama", "kum"]
DIGITAL = ["dijital kod", "gamepass", "game pass", "lisans", "abonelik", "e-pin"]
CLOTHING = ["tişört", "tshirt", "gömlek", "pantolon", "ayakkabı", "mont", "çorap", "elbise"]


def tr_lower(s: str) -> str:
    return (s or "").replace("İ", "i").replace("I", "ı").lower()


def bucket(title: str) -> str:
    t = tr_lower(title)
    if any(w in t for w in DIGITAL):
        return "digital"
    if any(w in t for w in CLOTHING):
        return "clothing"
    if any(re.search(rf"\b{re.escape(w)}\b", t) for w in CONSUMABLE):
        return "consumable"
    return "durable"


def mail_lines(path: Path) -> list[str]:
    b = path.read_bytes()
    n, rest = b.split(b"\n", 1)
    msg = email.message_from_bytes(rest[: int(n)], policy=policy.default)
    part = msg.get_body(("html", "plain"))
    try:
        h = part.get_content() if part else ""
    except Exception:
        return []
    h = re.sub(r"(?is)<(script|style).*?</\1>", "", h)
    t = unescape(re.sub(r"<[^>]+>", "\n", h))
    return [x.strip() for x in t.split("\n") if x.strip()]


def when(s: str | None) -> dt.datetime | None:
    s = (s or "").strip()
    for fmt in ("%Y-%m-%d %H:%M:%S", "%d/%m/%Y %H:%M:%S", "%d/%m/%Y %H:%M", "%Y-%m-%d %H:%M"):
        try:
            return dt.datetime.strptime(s[: len(dt.datetime(2000, 1, 1).strftime(fmt))], fmt)
        except ValueError:
            continue
    return None


def money(s: str | None) -> float | None:
    m = re.search(r"([\d.,]+)\s*TL", s or "")
    if not m:
        return None
    v = m.group(1)
    if "," in v:
        v = v.replace(".", "").replace(",", ".")
    return float(v)


def blocks(lines: list[str]) -> list[dict]:
    """One dict per "(# <id>)" item block: title, price text, time, sale code, seller."""
    out = []
    for i, line in enumerate(lines):
        m = re.match(r"^\(#\s*(\d+)\)$", line)
        if not m:
            continue
        b = {"id": m.group(1), "title": lines[i - 1] if i else ""}
        for j in range(i + 1, min(i + 30, len(lines))):
            if lines[j].startswith("(#"):
                break
            label = tr_lower(lines[j]).strip(" :")
            nxt = lines[j + 1] if j + 1 < len(lines) else ""
            if nxt == ":" and j + 2 < len(lines):
                nxt = ":" + lines[j + 2]
            if not nxt.startswith(":"):
                continue
            val = nxt.lstrip(": ").strip()
            if label in PRICE and "price" not in b:
                b["price"] = val
            elif label in WHEN and "when" not in b:
                b["when"] = val
            elif label in CODE and "code" not in b:
                b["code"] = val
            elif label in SELLER and "seller" not in b:
                b["seller"] = val
        out.append(b)
    return out


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--dir", default="~/.ev/purchases/gittigidiyor")
    args = ap.parse_args()
    root = Path(os.path.expanduser(args.dir))
    messages = json.loads((root / "messages.json").read_text())
    messages.sort(key=lambda m: m["date_received"])

    sales: list[dict] = []

    def find(b: dict, at: dt.datetime | None) -> dict | None:
        same = [s for s in sales if s["id"] == b["id"]]
        if b.get("code"):
            hit = [s for s in same if s.get("code") == b["code"]]
            if hit:
                return hit[0]
        if at:
            hit = [s for s in same if s["at"] and s["at"].replace(second=0) == at.replace(second=0)]
            if hit:
                return hit[0]
        return same[0] if len(same) == 1 and not (at and same[0]["at"]) else None

    # Payment mails first: each item block is a sale.
    for m in messages:
        if m["kind"] != "paid" or not m.get("file"):
            continue
        path = root / m["file"]
        for b in blocks(mail_lines(path)):
            at = when(b.get("when"))
            s = find(b, at)
            if s is None:
                s = {"id": b["id"], "at": at, "raw": path, "mail_at": m["date_received"]}
                sales.append(s)
            for k in ("title", "price", "code"):
                if b.get(k) and not s.get(k):
                    s[k] = b[k]
            if at and not s["at"]:
                s["at"] = at
            # A mail with the time is the better raw record.
            if b.get("when"):
                s["raw"] = path

    # Everything else adds the seller and the outcome.
    for m in messages:
        if m["kind"] == "paid" or not m.get("file"):
            continue
        path = root / m["file"]
        lines = mail_lines(path)
        text = " ".join(lines)
        for b in blocks(lines):
            s = find(b, when(b.get("when")))
            if s is None:
                continue
            seller = b.get("seller")
            mm = re.search(r"(\S+) adlı satıcı", text)
            if not seller and mm and m["kind"] == "shipped":
                seller = mm.group(1)
            if seller and not s.get("seller"):
                s["seller"] = seller
            if m["kind"] == "return_started":
                s["returned"] = True
        sm = re.match(r"^([\w-]+): (?:RE|Re)?:?\s*(\d+)\s*:", m["subject"])
        if m["kind"] == "seller_message" and sm:
            same = [s for s in sales if s["id"] == sm.group(2)]
            if len(same) == 1 and not same[0].get("seller"):
                same[0]["seller"] = sm.group(1)

    out = sys.stdout
    for s in sales:
        at = s["at"] or dt.datetime.fromtimestamp(s["mail_at"])
        qty = re.search(r"\((\d+) adet x", s.get("price") or "")
        paid = money(s.get("price"))
        title = s.get("title") or s["id"]
        rec = {
            "type": "purchase",
            "source": SOURCE,
            "key": f"{s['id']}:{at.strftime('%Y%m%d%H%M')}",
            "shop": "GittiGidiyor",
            "merchant": s.get("seller"),
            "order": s.get("code"),
            "sku": s["id"],
            "name": title,
            "ordered_at": at.strftime("%Y-%m-%d"),
            "qty": int(qty.group(1)) if qty else 1,
            "paid": f"{paid:.2f}" if paid is not None else None,
            "currency": "TRY",
            "status": "returned" if s.get("returned") else "delivered",
            "bucket": bucket(title),
            "raw": str(s["raw"]),
        }
        out.write(json.dumps({k: v for k, v in rec.items() if v is not None}, ensure_ascii=False))
        out.write("\n")


if __name__ == "__main__":
    main()
