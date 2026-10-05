//! Photos: stored copies, crops cut from one photo for many records, and marked copies that are
//! shown and forgotten.

use super::*;

/// Removes the files in `dir` last changed more than `age` ago; a scratch folder's housekeeping,
/// so whatever fails is left alone.
fn prune_older(dir: &Path, age: Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let now = std::time::SystemTime::now();
    for e in entries.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| now.duration_since(t).ok())
            .is_some_and(|d| d > age);
        if old {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

/// Where a marked copy of `file` goes when no place is given: `<temp>/ev-marks`, a scratch folder
/// whose files older than a day are removed first.
fn scratch_copy(file: &Path) -> PathBuf {
    scratch_path(file, "marked")
}

/// `<temp>/ev-marks/<stem>-<what>-<ms>.jpg`, pruning the folder's day-old files first.
fn scratch_path(file: &Path, what: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("ev-marks");
    prune_older(&dir, Duration::from_secs(24 * 3600));
    let stem = file
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "photo".into());
    let ms = chrono::Utc::now().timestamp_millis();
    dir.join(format!("{stem}-{what}-{ms}.jpg"))
}

/// A label's leading number and the rest: `2 → A6` is (2, " → A6"), `3` is (3, ""). A label
/// whose digits run into letters (`12A`) is no number.
fn leading_number(label: &str) -> Option<(usize, &str)> {
    let end = label
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(label.len());
    let rest = &label[end..];
    if end == 0 || !(rest.is_empty() || rest.starts_with(char::is_whitespace)) {
        return None;
    }
    Some((label[..end].parse().ok()?, rest))
}

/// What a crop is called on a contact sheet: its cell when it stands in a grid (`B3`), else its
/// code, else its `#id`.
fn tile_label(conn: &Connection, id: i64) -> Result<String> {
    if let Some(cells) = crate::grid::cells_of(conn, id)? {
        return Ok(cells.anchor());
    }
    Ok(load(conn, id)?.code.unwrap_or_else(|| format!("#{id}")))
}

/// The whole photo with a numbered red frame on each crop, numbered with `numbers` in the order
/// given, and the legend that says which record each number is: `[{n, ref, crop}]`. The person
/// sees what was recognised and answers by number. A copy that cannot be drawn leaves `marked`
/// empty: by then a cut is recorded already.
fn numbered(
    conn: &Connection,
    name: &Path,
    photo: &Path,
    crops: &[(i64, crate::Crop)],
    numbers: &[usize],
) -> Result<(Option<String>, Vec<Value>)> {
    let legend = crops
        .iter()
        .zip(numbers)
        .map(|((id, c), n)| {
            Ok(json!({ "n": n, "ref": brief_json(conn, *id)?, "crop": c.to_string() }))
        })
        .collect::<Result<Vec<_>>>()?;
    if crops.is_empty() {
        return Ok((None, legend));
    }
    let shapes: Vec<(String, crate::photo::Shape)> = crops
        .iter()
        .zip(numbers)
        .map(|((_, c), n)| (n.to_string(), crate::photo::Shape::Rect(*c)))
        .collect();
    let out = scratch_path(name, "numbered");
    let marked = crate::photo::draw_marks(photo, &shapes, &out)
        .ok()
        .map(|()| out.to_string_lossy().into_owned());
    Ok((marked, legend))
}

impl Inventory {
    /// Copies a photo into the store and attaches it to a node; with `crop`, attaches only
    /// the cut-out and remembers the stored original it came from.
    pub fn photo_add(
        &mut self,
        reference: &str,
        file: &Path,
        crop: Option<crate::Crop>,
        note: Option<&str>,
    ) -> Result<Value> {
        self.photo_add_with(reference, file, crop, note, false)
    }

    /// What `photo_add_with` refuses, checked without attaching anything, so a batch can check
    /// every photo first: the record must resolve, and unless `cut_or_whole` (a crop, or
    /// `--whole`) the photo must not be attached whole to another node. Returns the record's id
    /// and the photo as stored (the store is content-addressed: storing twice is one file).
    pub fn photo_add_check(
        &self,
        reference: &str,
        file: &Path,
        cut_or_whole: bool,
    ) -> Result<(i64, PathBuf)> {
        let id = resolve(&self.conn, reference, false)?;
        let original = crate::photo::store_file(&self.photo_dir, file)?;
        if !cut_or_whole {
            let others = ids(
                &self.conn,
                "SELECT DISTINCT p.node_id FROM photos p JOIN nodes n ON n.id = p.node_id
                  WHERE p.path = ev_store(?1) AND p.crop IS NULL AND p.node_id != ?2 AND n.state != 'gone'
                  ORDER BY p.node_id",
                params![original.to_string_lossy(), id],
            )?;
            if !others.is_empty() {
                let nodes = others
                    .iter()
                    .map(|o| brief_json(&self.conn, *o))
                    .collect::<Result<Vec<_>>>()?;
                return Err(refused(
                    format!(
                        "this photo is already attached whole to {} other node(s); attach a --crop \
                         of the part that shows node {id}, or pass --whole if the whole view is meant",
                        others.len()
                    ),
                    json!({ "attached_to": nodes }),
                ));
            }
        }
        Ok((id, original))
    }

    /// Adds a photo. A whole (uncropped) photo already attached whole to another node is
    /// refused unless `whole` is set: a group photo belongs to the place, and the things in it
    /// get crops. The mistake this stops — one drawer photo on nine items — only shows once
    /// someone opens them.
    pub fn photo_add_with(
        &mut self,
        reference: &str,
        file: &Path,
        crop: Option<crate::Crop>,
        note: Option<&str>,
        whole: bool,
    ) -> Result<Value> {
        let (id, original) = self.photo_add_check(reference, file, crop.is_some() || whole)?;
        let (stored, source) = match crop {
            Some(c) => (
                crate::photo::store_crop(&self.photo_dir, &original, c)?,
                Some(original.to_string_lossy().into_owned()),
            ),
            None => (original, None),
        };
        let stored = stored.to_string_lossy().into_owned();
        let tx = self.conn.transaction()?;
        let next: i64 = tx.query_row(
            "SELECT COALESCE(MAX(position) + 1, 0) FROM photos WHERE node_id = ?1",
            [id],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO photos (node_id, position, path, source, crop, note, added_at)
             VALUES (?1, ?2, ev_store(?3), ev_store(?4), ?5, ?6, ?7)",
            params![
                id,
                next,
                stored,
                source,
                crop.map(|c| c.to_string()),
                note.map(str::trim),
                now()
            ],
        )?;
        touch(&tx, id)?;
        event(
            &tx,
            id,
            "photo",
            json!({ "path": stored, "crop": crop.map(|c| c.to_string()) }),
        )?;
        tx.commit()?;
        self.photo_list(&id.to_string())
    }

    /// Cuts one photo up among several nodes in one step: a crop for each `(reference, crop)`,
    /// and the whole photo on `place` when given (the drawer or box it shows). Every reference
    /// is resolved and every crop cut before anything is recorded, and the records go in one
    /// transaction, so a typo leaves nothing half attached. The whole-photo rule of
    /// `photo_add` applies to `place`.
    pub fn photo_cut(
        &mut self,
        file: &Path,
        place: Option<&str>,
        crops: &[(String, crate::Crop)],
        note: Option<&str>,
        grid: Option<&crate::GridCorners>,
    ) -> Result<Value> {
        self.photo_cut_in(file, place, crops, note, grid, false)
    }

    /// The numbers a copy of `file` with `count` frames draws: in the series on screen when
    /// `series` (spec/focus-stack.md), so a batch's numbers never repeat; else 1…
    fn frame_numbers(&self, file: &Path, count: usize, series: bool) -> Result<Vec<usize>> {
        if series {
            self.focus_numbers(file, count)
        } else {
            Ok((1..=count).collect())
        }
    }

    /// `photo_cut` whose numbered copy takes its numbers in the series on screen when `series`:
    /// the numbers this photo was marked with there, then the series' next free ones.
    pub fn photo_cut_in(
        &mut self,
        file: &Path,
        place: Option<&str>,
        crops: &[(String, crate::Crop)],
        note: Option<&str>,
        grid: Option<&crate::GridCorners>,
        series: bool,
    ) -> Result<Value> {
        if place.is_none() && crops.is_empty() {
            return Err(Error::Usage(
                "give at least one <ref>=x,y,w,h, or --place <ref>".into(),
            ));
        }
        if grid.is_some() && place.is_none() {
            return Err(Error::Usage(
                "--grid reads the boxes of the --place grid; give --place too".into(),
            ));
        }
        let place = place.map(|p| resolve(&self.conn, p, false)).transpose()?;
        let mut targets = crops
            .iter()
            .map(|(r, c)| Ok((resolve(&self.conn, r, false)?, *c)))
            .collect::<Result<Vec<_>>>()?;
        // Every box in the place's grid gets its crop from the grid's corners, unless a crop was
        // given for it by hand.
        if let (Some(pid), Some(corners)) = (place, grid) {
            for (id, c) in crate::grid::grid_crops(&self.conn, pid, corners)? {
                if !targets.iter().any(|(t, _)| *t == id) {
                    targets.push((id, c));
                }
            }
        }
        let original = crate::photo::store_file(&self.photo_dir, file)?;
        let original_text = original.to_string_lossy().into_owned();
        if let Some(pid) = place {
            let others = ids(
                &self.conn,
                "SELECT DISTINCT p.node_id FROM photos p JOIN nodes n ON n.id = p.node_id
                  WHERE p.path = ev_store(?1) AND p.crop IS NULL AND p.node_id != ?2 AND n.state != 'gone'
                  ORDER BY p.node_id",
                params![original_text, pid],
            )?;
            if !others.is_empty() {
                let nodes = others
                    .iter()
                    .map(|o| brief_json(&self.conn, *o))
                    .collect::<Result<Vec<_>>>()?;
                return Err(refused(
                    format!(
                        "this photo is already attached whole to {} other node(s)",
                        others.len()
                    ),
                    json!({ "attached_to": nodes }),
                ));
            }
        }
        // The whole photo keeps the grid's corners, so its cells can be marked by name later.
        type Row = (
            i64,
            String,
            Option<String>,
            Option<crate::Crop>,
            Option<String>,
        );
        let mut rows: Vec<Row> = Vec::new();
        if let Some(pid) = place {
            let corners = grid.map(|g| g.to_string());
            rows.push((pid, original_text.clone(), None, None, corners));
        }
        for (id, c) in targets {
            let cut = crate::photo::store_crop(&self.photo_dir, &original, c)?;
            rows.push((
                id,
                cut.to_string_lossy().into_owned(),
                Some(original_text.clone()),
                Some(c),
                None,
            ));
        }
        let tx = self.conn.transaction()?;
        let mut attached = Vec::new();
        for (id, stored, source, crop, corners) in &rows {
            let next: i64 = tx.query_row(
                "SELECT COALESCE(MAX(position) + 1, 0) FROM photos WHERE node_id = ?1",
                [id],
                |r| r.get(0),
            )?;
            tx.execute(
                "INSERT INTO photos (node_id, position, path, source, crop, note, added_at, grid)
                 VALUES (?1, ?2, ev_store(?3), ev_store(?4), ?5, ?6, ?7, ?8)",
                params![
                    id,
                    next,
                    stored,
                    source,
                    crop.map(|c| c.to_string()),
                    note.map(str::trim),
                    now(),
                    corners
                ],
            )?;
            touch(&tx, *id)?;
            event(
                &tx,
                *id,
                "photo",
                json!({ "path": stored, "crop": crop.map(|c| c.to_string()) }),
            )?;
            let mut b = brief_json(&tx, *id)?;
            b["photo"] = json!(next + 1);
            b["crop"] = json!(crop.map(|c| c.to_string()));
            b["path"] = json!(stored);
            attached.push(b);
        }
        tx.commit()?;
        // Every crop small on one sheet, to check the cut at a glance. The cut is recorded
        // already: a sheet that cannot be drawn is reported as missing, not as a failed cut.
        let tiles = rows
            .iter()
            .filter_map(|(id, _, _, crop, _)| crop.map(|c| (*id, c)))
            .map(|(id, c)| Ok((tile_label(&self.conn, id)?, c)))
            .collect::<Result<Vec<_>>>()?;
        let sheet = (!tiles.is_empty())
            .then(|| {
                let out = scratch_path(file, "sheet");
                crate::photo::contact_sheet(&original, &tiles, &out)
                    .ok()
                    .map(|()| out.to_string_lossy().into_owned())
            })
            .flatten();
        let crops: Vec<(i64, crate::Crop)> = rows
            .iter()
            .filter_map(|(id, _, _, crop, _)| crop.map(|c| (*id, c)))
            .collect();
        let numbers = self.frame_numbers(file, crops.len(), series)?;
        let (marked, legend) = numbered(&self.conn, file, &original, &crops, &numbers)?;
        Ok(json!({
            "attached": attached,
            "sheet": sheet,
            "marked": marked,
            "legend": legend,
        }))
    }

    /// What `photo_cut` would cut, drawn instead of cut: each placed box of the `place` grid
    /// framed on its cells and labelled with its back-left cell, and each crop named by hand
    /// framed and labelled with its reference. Nothing is stored or attached; the copy goes to
    /// `out` or the scratch folder of `photo_mark`. For checking grid corners by eye before
    /// cutting.
    pub fn photo_cut_preview(
        &self,
        file: &Path,
        place: Option<&str>,
        crops: &[(String, crate::Crop)],
        grid: Option<&crate::GridCorners>,
        out: Option<&Path>,
    ) -> Result<Value> {
        self.photo_cut_preview_in(file, place, crops, grid, out, false)
    }

    /// `photo_cut_preview` whose numbered copy takes its numbers in the series on screen when
    /// `series` (spec/focus-stack.md).
    pub fn photo_cut_preview_in(
        &self,
        file: &Path,
        place: Option<&str>,
        crops: &[(String, crate::Crop)],
        grid: Option<&crate::GridCorners>,
        out: Option<&Path>,
        series: bool,
    ) -> Result<Value> {
        use crate::photo::Shape;
        if !file.is_file() {
            return Err(Error::NotFound(format!("no file {}", file.display())));
        }
        let mut shapes = Vec::new();
        let mut tiles: Vec<(i64, String, crate::Crop)> = Vec::new();
        for (r, c) in crops {
            let id = resolve(&self.conn, r, false)?;
            shapes.push((r.clone(), Shape::Rect(*c)));
            tiles.push((id, r.clone(), *c));
        }
        if let Some(corners) = grid {
            let Some(p) = place else {
                return Err(Error::Usage(
                    "--grid reads the boxes of the --place grid; give --place too".into(),
                ));
            };
            let pid = resolve(&self.conn, p, false)?;
            for (_, cells) in crate::grid::placed(&self.conn, pid)? {
                let quad = crate::grid::cells_quad(&self.conn, pid, corners, &cells)?;
                shapes.push((cells.anchor(), Shape::Quad(quad)));
            }
            // The crops the cut would make, as the cut makes them: a crop named by hand wins.
            for (id, c) in crate::grid::grid_crops(&self.conn, pid, corners)? {
                if !tiles.iter().any(|(t, _, _)| *t == id) {
                    tiles.push((id, tile_label(&self.conn, id)?, c));
                }
            }
        }
        if shapes.is_empty() {
            return Err(Error::Usage(
                "nothing to preview: give <ref>=x,y,w,h, or --place with --grid".into(),
            ));
        }
        let out = out.map_or_else(|| scratch_copy(file), Path::to_path_buf);
        crate::photo::draw_marks(file, &shapes, &out)?;
        let crops: Vec<(i64, crate::Crop)> = tiles.iter().map(|(id, _, c)| (*id, *c)).collect();
        let numbers = self.frame_numbers(file, crops.len(), series)?;
        let (marked, legend) = numbered(&self.conn, file, file, &crops, &numbers)?;
        let tiles: Vec<(String, crate::Crop)> =
            tiles.into_iter().map(|(_, label, c)| (label, c)).collect();
        let sheet = scratch_path(file, "sheet");
        crate::photo::contact_sheet(file, &tiles, &sheet)?;
        Ok(json!({
            "preview": out.to_string_lossy(),
            "framed": shapes.len(),
            "sheet": sheet.to_string_lossy(),
            "marked": marked,
            "legend": legend,
        }))
    }

    /// Draws marks on a copy of a photo, to show which thing is meant and where it goes. The
    /// copy is temporary: it is not stored, not attached and leaves no history; it goes to
    /// `out`, or to a scratch folder (`<temp>/ev-marks`) whose files older than a day are
    /// removed on each call. `target` is a photo file, or a node whose newest whole photo is
    /// marked. A mark is `(label, spec)`: `x,y,w,h` in fractions of the upright photo, or cells
    /// like `A6` / `A6-B6` of the node's grid, located through the grid corners kept with the
    /// photo when it was cut (`ev photo cut --grid`) or given here.
    pub fn photo_mark(
        &self,
        target: &str,
        marks: &[(String, String)],
        corners: Option<&crate::GridCorners>,
        out: Option<&Path>,
    ) -> Result<Value> {
        self.photo_mark_in(target, marks, corners, out, None)
    }

    /// `photo_mark` for the series on screen (spec/focus-stack.md): a numbered label (`1`,
    /// `2 → A6`) counts this photo's frames, and takes the frame's number in the series — the
    /// photo's own numbers when it is there already, then the next free ones — so every number
    /// on screen means one frame until the person closes the series. The marks
    /// returned carry the numbers drawn, and `frames` lists them for the series. `keep` draws the
    /// numbers as given: marks that point at frames already numbered (where frame 4 goes).
    pub fn photo_mark_numbered(
        &self,
        target: &str,
        marks: &[(String, String)],
        corners: Option<&crate::GridCorners>,
        out: Option<&Path>,
        keep: bool,
    ) -> Result<Value> {
        self.photo_mark_in(target, marks, corners, out, Some(keep))
    }

    fn photo_mark_in(
        &self,
        target: &str,
        marks: &[(String, String)],
        corners: Option<&crate::GridCorners>,
        out: Option<&Path>,
        numbered: Option<bool>,
    ) -> Result<Value> {
        use crate::photo::Shape;
        if marks.is_empty() {
            return Err(Error::Usage(
                "give at least one <label>=x,y,w,h or <label>=<cell>".into(),
            ));
        }
        let by_cell = |spec: &str| !spec.contains(',');
        let (file, holder, stored) = if Path::new(target).is_file() {
            (PathBuf::from(target), None, None)
        } else {
            let id = resolve(&self.conn, target, false)?;
            // A cell needs the grid's corners: the newest whole photo that kept them, unless
            // they are given; otherwise the newest whole photo.
            let want_grid = corners.is_none() && marks.iter().any(|(_, s)| by_cell(s));
            let sql = if want_grid {
                "SELECT ev_file(path), grid FROM photos WHERE node_id = ?1 AND crop IS NULL
                   AND grid IS NOT NULL ORDER BY position DESC LIMIT 1"
            } else {
                "SELECT ev_file(path), grid FROM photos WHERE node_id = ?1 AND crop IS NULL
                   ORDER BY position DESC LIMIT 1"
            };
            let row: Option<(String, Option<String>)> = self
                .conn
                .query_row(sql, [id], |r| Ok((r.get(0)?, r.get(1)?)))
                .optional()?;
            let Some((path, grid)) = row else {
                let why = if want_grid {
                    "no photo of it kept its grid corners; give --grid, or cut the next one with --grid"
                } else {
                    "it has no whole photo to mark"
                };
                return Err(refused(why, json!({ "node": brief_json(&self.conn, id)? })));
            };
            (PathBuf::from(path), Some(id), grid)
        };
        let mut marks = marks.to_vec();
        // The labels' numbers count this photo's frames: on a first mark, in their order, each
        // takes the series' next free number, so a lone `2` (the agent counting across photos)
        // is not left a gap before it; on a photo marked again, a label keeps its number. The
        // label each frame was given is kept with it for that.
        let mut asked: Vec<Option<usize>> = vec![None; marks.len()];
        if numbered == Some(false) {
            let mut given: Vec<usize> = marks
                .iter()
                .filter_map(|(l, _)| leading_number(l).map(|n| n.0))
                .collect();
            given.sort_unstable();
            given.dedup();
            let numbers = self.focus_numbers_by_label(&file, &given)?;
            for ((label, _), asked) in marks.iter_mut().zip(&mut asked) {
                if let Some((k, rest)) = leading_number(label) {
                    let rank = given.iter().position(|g| *g == k).unwrap_or(0);
                    *label = format!("{}{rest}", numbers[rank]);
                    *asked = Some(k);
                }
            }
        }
        let stored: Option<crate::GridCorners> = stored.map(|g| g.parse()).transpose()?;
        let corners = corners.or(stored.as_ref());
        let mut shapes = Vec::new();
        for (text, spec) in &marks {
            let shape = if by_cell(spec) {
                let cells = crate::grid::Cells::parse(spec)?;
                let (Some(h), Some(c)) = (holder, corners) else {
                    return Err(Error::Usage(format!(
                        "`{spec}` is a cell: mark a place with a grid (by its code), not a file"
                    )));
                };
                Shape::Quad(crate::grid::cells_quad(&self.conn, h, c, &cells)?)
            } else {
                Shape::Rect(spec.parse()?)
            };
            shapes.push((text.clone(), shape));
        }
        let out = out.map_or_else(|| scratch_copy(&file), Path::to_path_buf);
        crate::photo::draw_marks(&file, &shapes, &out)?;
        let mut v = json!({
            "marked": out.to_string_lossy(),
            "source": file.to_string_lossy(),
            "marks": marks.iter().map(|(l, s)| json!({ "label": l, "at": s })).collect::<Vec<_>>(),
        });
        if numbered.is_some() {
            v["frames"] = marks
                .iter()
                .zip(&asked)
                .filter_map(|((l, s), asked)| {
                    let mut f = json!({ "n": leading_number(l)?.0, "at": s });
                    if numbered == Some(true) {
                        f["kept"] = json!(true);
                    }
                    if let Some(k) = asked {
                        f["label"] = json!(k);
                    }
                    Some(f)
                })
                .collect();
        }
        Ok(v)
    }

    pub fn photo_list(&self, reference: &str) -> Result<Value> {
        let id = resolve(&self.conn, reference, true)?;
        let mut stmt = self.conn.prepare(
            "SELECT ev_file(path), ev_file(source), crop, note, added_at FROM photos WHERE node_id = ?1 ORDER BY position",
        )?;
        let photos = stmt
            .query_map([id], |r| {
                let path: String = r.get(0)?;
                Ok(json!({
                    "path": path,
                    "exists": Path::new(&path).exists(),
                    "source": r.get::<_, Option<String>>(1)?,
                    "crop": r.get::<_, Option<String>>(2)?,
                    "note": r.get::<_, Option<String>>(3)?,
                    "added_at": r.get::<_, Option<String>>(4)?,
                }))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let photos: Vec<Value> = photos
            .into_iter()
            .enumerate()
            .map(|(i, mut p)| {
                p["n"] = json!(i + 1);
                p
            })
            .collect();
        Ok(json!({ "node": brief_json(&self.conn, id)?, "photos": photos }))
    }

    /// Detaches the n-th photo (1-based); the stored file stays, other nodes may share it. The
    /// history keeps what was detached (`photo_remove`), as it keeps what was attached.
    /// `file` turned `degrees` clockwise, as a temporary copy to add or cut instead of it
    /// (`--rotate`, spec/rotate.md): what is stored is the turned photo, and coordinates given
    /// with it are fractions of the turned one.
    pub fn turned_copy(&self, file: &Path, degrees: u16) -> Result<PathBuf> {
        let turns = crate::photo::quarter_turns(degrees)?;
        let bytes = crate::photo::turned_jpeg(file, turns)?;
        let out = scratch_path(file, "turned");
        if let Some(dir) = out.parent() {
            std::fs::create_dir_all(dir).map_err(|e| Error::Internal(e.to_string()))?;
        }
        std::fs::write(&out, bytes).map_err(|e| Error::Internal(e.to_string()))?;
        Ok(out)
    }

    /// Turns a node's `n`-th photo `degrees` clockwise for good, with everything cut from it
    /// (spec/rotate.md): the photo is stored turned; every record holding it whole gets the
    /// turned one, its grid corners turned; every crop cut from it, on any record, is turned
    /// with it and cut again. A crop turns its source photo. Files nothing uses any more are
    /// deleted.
    pub fn photo_rotate(&mut self, reference: &str, n: usize, degrees: u16) -> Result<Value> {
        let turns = crate::photo::quarter_turns(degrees)?;
        let id = resolve(&self.conn, reference, false)?;
        let positions = ids(
            &self.conn,
            "SELECT position FROM photos WHERE node_id = ?1 ORDER BY position",
            [id],
        )?;
        let pos = *n
            .checked_sub(1)
            .and_then(|i| positions.get(i))
            .ok_or_else(|| {
                Error::NotFound(format!(
                    "photo {n} does not exist; it has {}",
                    positions.len()
                ))
            })?;
        let (path, source): (String, Option<String>) = self.conn.query_row(
            "SELECT ev_file(path), ev_file(source) FROM photos WHERE node_id = ?1 AND position = ?2",
            params![id, pos],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        // A crop is sideways because the photo it was cut from is: that photo turns.
        let old = source.unwrap_or(path);
        let new = crate::photo::store_bytes(
            &self.photo_dir,
            &crate::photo::turned_jpeg(Path::new(&old), turns)?,
            "jpg",
        )?;
        let new_text = new.to_string_lossy().into_owned();
        type Whole = (i64, i64, Option<String>);
        let whole: Vec<Whole> = {
            let mut stmt = self.conn.prepare(
                "SELECT node_id, position, grid FROM photos
                  WHERE path = ev_store(?1) AND crop IS NULL ORDER BY node_id, position",
            )?;
            stmt.query_map([&old], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
                .collect::<rusqlite::Result<_>>()?
        };
        type Cut = (i64, i64, String, String);
        let cuts: Vec<Cut> = {
            let mut stmt = self.conn.prepare(
                "SELECT node_id, position, ev_file(path), crop FROM photos
                  WHERE source = ev_store(?1) AND crop IS NOT NULL ORDER BY node_id, position",
            )?;
            stmt.query_map([&old], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
                .collect::<rusqlite::Result<_>>()?
        };
        // Every crop is cut again before anything is recorded.
        let mut recut = Vec::with_capacity(cuts.len());
        for (node, position, file, crop) in &cuts {
            let c = crop.parse::<crate::Crop>()?.turned(turns);
            let cut = crate::photo::store_crop(&self.photo_dir, &new, c)?;
            recut.push((*node, *position, file.clone(), cut, c));
        }
        let tx = self.conn.transaction()?;
        let mut nodes = Vec::new();
        for (node, position, grid) in &whole {
            let grid = grid
                .as_deref()
                .map(str::parse::<crate::GridCorners>)
                .transpose()?
                .map(|g| g.turned(turns).to_string());
            tx.execute(
                "UPDATE photos SET path = ev_store(?1), grid = ?2 WHERE node_id = ?3 AND position = ?4",
                params![new_text, grid, node, position],
            )?;
            nodes.push(*node);
        }
        for (node, position, _, cut, c) in &recut {
            tx.execute(
                "UPDATE photos SET path = ev_store(?1), source = ev_store(?2), crop = ?3
                  WHERE node_id = ?4 AND position = ?5",
                params![
                    cut.to_string_lossy(),
                    new_text,
                    c.to_string(),
                    node,
                    position
                ],
            )?;
            nodes.push(*node);
        }
        nodes.sort_unstable();
        nodes.dedup();
        for node in &nodes {
            event(
                &tx,
                *node,
                "photo_rotate",
                json!({ "from": old, "to": new_text, "degrees": degrees }),
            )?;
        }
        // The old photo and its old crops, once nothing points at them.
        let mut unused = Vec::new();
        for f in std::iter::once(old.clone()).chain(recut.iter().map(|r| r.2.clone())) {
            let left: i64 = tx.query_row(
                "SELECT COUNT(*) FROM photos WHERE path = ev_store(?1) OR source = ev_store(?1)",
                [&f],
                |r| r.get(0),
            )?;
            if left == 0 && Path::new(&f).starts_with(&self.photo_dir) && f != new_text {
                unused.push(f);
            }
        }
        tx.commit()?;
        unused.sort();
        unused.dedup();
        let deleted: Vec<String> = unused
            .into_iter()
            .filter(|f| std::fs::remove_file(f).is_ok())
            .collect();
        let mut v = self.photo_list(&id.to_string())?;
        v["rotated"] = json!({
            "degrees": degrees,
            "records": nodes
                .iter()
                .map(|n| brief_json(&self.conn, *n))
                .collect::<Result<Vec<_>>>()?,
            "deleted_files": deleted,
        });
        Ok(v)
    }

    pub fn photo_remove(&mut self, reference: &str, n: usize) -> Result<Value> {
        let id = resolve(&self.conn, reference, false)?;
        let positions = ids(
            &self.conn,
            "SELECT position FROM photos WHERE node_id = ?1 ORDER BY position",
            [id],
        )?;
        let pos = *n
            .checked_sub(1)
            .and_then(|i| positions.get(i))
            .ok_or_else(|| {
                Error::NotFound(format!(
                    "photo {n} does not exist; it has {}",
                    positions.len()
                ))
            })?;
        let tx = self.conn.transaction()?;
        let (path, crop, note): (String, Option<String>, Option<String>) = tx.query_row(
            "SELECT ev_file(path), crop, note FROM photos WHERE node_id = ?1 AND position = ?2",
            params![id, pos],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        tx.execute(
            "DELETE FROM photos WHERE node_id = ?1 AND position = ?2",
            params![id, pos],
        )?;
        event(
            &tx,
            id,
            "photo_remove",
            json!({ "path": path, "crop": crop, "note": note, "n": n }),
        )?;
        // A crop no record uses any more is a file nobody can reach: cut again from its source
        // when needed. A whole photo stays, since it may be the only copy of what was taken.
        let unused = crop.is_some()
            && Path::new(&path).starts_with(&self.photo_dir)
            && tx.query_row(
                "SELECT COUNT(*) FROM photos WHERE path = ev_store(?1) OR source = ev_store(?1)",
                [&path],
                |r| r.get::<_, i64>(0),
            )? == 0;
        tx.commit()?;
        let mut v = self.photo_list(&id.to_string())?;
        if unused && std::fs::remove_file(&path).is_ok() {
            v["deleted_file"] = json!(path);
        }
        Ok(v)
    }

    /// Copies every photo still referenced outside the store into it.
    pub fn photo_adopt(&mut self) -> Result<Value> {
        let dir = self.photo_dir.to_string_lossy().into_owned();
        let mut stmt = self.conn.prepare(
            "SELECT node_id, position, ev_file(path) FROM photos ORDER BY node_id, position",
        )?;
        let rows: Vec<(i64, i64, String)> = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        drop(stmt);
        let (mut adopted, mut missing) = (0, Vec::new());
        for (node, pos, path) in rows {
            if path.starts_with(&dir) {
                continue;
            }
            if !Path::new(&path).exists() {
                missing.push(json!({ "node": node, "path": path }));
                continue;
            }
            let stored = crate::photo::store_file(&self.photo_dir, Path::new(&path))?;
            self.conn.execute(
                "UPDATE photos SET path = ev_store(?1), note = COALESCE(note, ?2) WHERE node_id = ?3 AND position = ?4",
                params![stored.to_string_lossy(), format!("adopted from {path}"), node, pos],
            )?;
            adopted += 1;
        }
        Ok(json!({ "adopted": adopted, "missing": missing, "store": dir }))
    }
}

#[cfg(test)]
mod scratch_tests {
    use std::time::{Duration, SystemTime};

    #[test]
    fn marked_copies_older_than_a_day_are_cleared_and_newer_ones_kept() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("old-marked.jpg");
        let new = dir.path().join("new-marked.jpg");
        std::fs::write(&old, b"x").unwrap();
        std::fs::write(&new, b"x").unwrap();
        let two_days_ago = SystemTime::now() - Duration::from_secs(2 * 24 * 3600);
        std::fs::File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(two_days_ago)
            .unwrap();
        super::prune_older(dir.path(), Duration::from_secs(24 * 3600));
        assert!(!old.exists());
        assert!(new.exists());
        // A folder that is not there yet is no error.
        super::prune_older(&dir.path().join("missing"), Duration::from_secs(1));
    }
}
