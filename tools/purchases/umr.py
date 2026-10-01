#!/usr/bin/env python3
"""Under My Roof -> ev purchase NDJSON (spec/purchases.md §6).

Under My Roof keeps its inventory in a Core Data SQLite store inside its sandbox. This adapter
never writes to it: it copies the store (with its WAL) to a temporary directory and reads the
copy. Every item not disposed of becomes one purchase line (`source: umr`, keyed by the item's
UUID) carrying what the app knew of its purchase, and hangs on it:

  valuation   the item's Value; the app keeps no date for it, so the item's last change is used
              and marked approximate
  coverage    each warranty (Under My Roof counts its terms in days, weeks, months or years)
  link        the "URL (Info)" custom field
  document    receipts (as invoices) and attachments, written out of the store when the app
              kept them inline

The "URL (Purchase)" custom field becomes the line's order page when it is one, else its product
page; `ev buy import` then joins the line to the shop's own line for the same order (same_as).
Files the store keeps inline are written under --files (outside every repository). Usage:

  tools/purchases/umr.py | ev buy import --stdin
"""
import argparse
import json
import os
import plistlib
import shutil
import sqlite3
import sys
import tempfile
from datetime import datetime, timedelta, timezone
from pathlib import Path

SOURCE = "umr"
STORE = (
    "~/Library/Containers/com.BinaryFormations.UnderMyRoof/Data/Library/Application Support/"
    "Inventories/Local"
)
CORE_DATA_EPOCH = datetime(2001, 1, 1, tzinfo=timezone.utc)
TERM_UNITS = {0: "d", 1: "w", 2: "m", 3: "y"}


def day(core_data_seconds):
    if core_data_seconds is None:
        return None
    return (CORE_DATA_EPOCH + timedelta(seconds=core_data_seconds)).date().isoformat()


def purchase_day(yyyymmdd):
    if not yyyymmdd:
        return None
    s = str(int(yyyymmdd))
    return f"{s[:4]}-{s[4:6]}-{s[6:8]}" if len(s) == 8 else None


def amount(x):
    return f"{round(x, 2):.2f}" if x and x > 0 else None


def custom_fields(conn):
    """Custom field id -> its name."""
    return {fid: name for fid, name in conn.execute("SELECT ZID, ZNAME FROM ZFIELDDEFINITION")}


def blob_file(conn, blob_pk, external_dir, out_dir, name, ext):
    """The file a binary-data row holds: the store's external file, or the inline bytes
    written out. Core Data marks external storage with a leading 0x02 and the file's UUID."""
    row = conn.execute("SELECT ZOBJECTDATA FROM ZBINARYDATA WHERE Z_PK = ?", (blob_pk,)).fetchone()
    if not row or not row[0]:
        return None
    data = row[0]
    if data[:1] == b"\x02":
        uuid = data[1:].split(b"\x00")[0].decode()
        path = external_dir / uuid
        return path if path.is_file() else None
    payload = data[1:] if data[:1] == b"\x01" else data
    out_dir.mkdir(parents=True, exist_ok=True)
    path = out_dir / f"{name}.{ext}"
    if not path.exists():
        path.write_bytes(payload)
    return path


def ext_of(path, fallback):
    with open(path, "rb") as f:
        head = f.read(8)
    if head.startswith(b"%PDF"):
        return "pdf"
    if head.startswith(b"\x89PNG"):
        return "png"
    if head[:3] == b"\xff\xd8\xff":
        return "jpg"
    return fallback


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--store", default=STORE, help="the folder holding DataStore.sql")
    ap.add_argument("--files", default="~/.ev/purchases/umr/files",
                    help="where files kept inline in the store are written")
    ap.add_argument("--currency", default="TRY", help="when an amount names none")
    args = ap.parse_args()
    store = Path(os.path.expanduser(args.store))
    files = Path(os.path.expanduser(args.files))
    external = store / ".DataStore_SUPPORT" / "_EXTERNAL_DATA"

    with tempfile.TemporaryDirectory() as tmp:
        for f in store.glob("DataStore.sql*"):
            shutil.copy2(f, tmp)
        conn = sqlite3.connect(f"file:{tmp}/DataStore.sql?mode=ro", uri=True)
        fields = custom_fields(conn)
        out = sys.stdout

        def emit(rec):
            out.write(json.dumps({k: v for k, v in rec.items() if v is not None},
                                 ensure_ascii=False))
            out.write("\n")

        items = conn.execute(
            """SELECT Z_PK, ZID, ZNAME, ZMAKE, ZMODEL, ZPRICE, ZPRICECURRENCYCODE, ZVALUE,
                      ZVALUECURRENCYCODE, ZPURCHASEDATE, ZPURCHASEDFROM, ZMODIFIEDON, ZQUANTITY,
                      ZCUSTOMFIELDDATA
                 FROM ZITEM WHERE ZDISPOSEDON IS NULL AND ZNAME IS NOT NULL ORDER BY Z_PK"""
        ).fetchall()
        for (pk, uid, name, make, model, price, price_cur, value, value_cur, bought, shop,
             modified, qty, custom) in items:
            urls = {}
            if custom:
                try:
                    for fid, v in plistlib.loads(custom).items():
                        if isinstance(v, str) and v.startswith("http"):
                            urls[fields.get(fid, "")] = v
                except Exception:
                    pass
            purchase_url = urls.get("URL (Purchase)")
            is_order = bool(purchase_url) and "order" in purchase_url.lower()
            qty = int(qty or 1)
            emit({
                "type": "purchase",
                "source": SOURCE,
                "key": uid,
                "shop": shop,
                "name": " ".join(x for x in [name, model if model and model not in name else None] if x),
                "brand": make,
                "ordered_at": purchase_day(bought),
                "qty": max(qty, 1),
                "paid": amount(price * max(qty, 1)) if price else None,
                "currency": (price_cur or args.currency) if price else None,
                "order_url": purchase_url if is_order else None,
                "product_url": purchase_url if purchase_url and not is_order else None,
                "bucket": "durable",
            })
            if value and value > 0:
                emit({
                    "type": "valuation",
                    "source": SOURCE,
                    "purchase": uid,
                    "amount": amount(value),
                    "currency": value_cur or args.currency,
                    "at": day(modified),
                    "approximate": True,
                    "from": "Under My Roof",
                })
            if urls.get("URL (Info)"):
                emit({"type": "link", "source": SOURCE, "purchase": uid,
                      "url": urls["URL (Info)"], "kind": "info"})
            for term, unit, start, issuer, number in conn.execute(
                """SELECT ZTERM, ZTERMTYPE, ZSTARTDATE, ZISSUER, ZPOLICYNUMBER FROM ZWARRANTY
                    WHERE ZITEM = ?""",
                (pk,),
            ):
                if not term:
                    continue
                emit({
                    "type": "coverage",
                    "source": SOURCE,
                    "purchase": uid,
                    "kind": "manufacturer",
                    "term": f"{term}{TERM_UNITS.get(unit, 'y')}",
                    "from": day(start) or purchase_day(bought),
                    "issuer": issuer,
                    "number": number,
                })
            for rpk, blob in conn.execute(
                "SELECT Z_PK, ZRECEIPTDATA FROM ZRECEIPT WHERE ZITEM = ?", (pk,)
            ):
                path = blob_file(conn, blob, external, files, f"receipt-{rpk}", "pdf")
                if path:
                    emit({"type": "document", "source": SOURCE, "kind": "invoice",
                          "file": str(path), "purchases": [uid]})
            for apk, blob, ext, original in conn.execute(
                """SELECT Z_PK, ZATTACHMENTDATA, ZEXTENSION, ZORIGINALFILENAME FROM ZATTACHMENT
                    WHERE ZITEM = ?""",
                (pk,),
            ):
                path = blob_file(conn, blob, external, files, f"attachment-{apk}", ext or "bin")
                if path:
                    emit({"type": "document", "source": SOURCE, "kind": "other",
                          "file": str(path), "purchases": [uid], "note": original})
        conn.close()


if __name__ == "__main__":
    main()
