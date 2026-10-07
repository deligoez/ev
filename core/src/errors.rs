//! The English sentence of every error with an id (spec/error-ids.md): one table, named
//! placeholders filled from the error's values when it is shown. The CLI keeps the Turkish
//! sentences by the same ids.

/// `(id, English template)`. An id, once released, is not renamed: agents and tests match on
/// it. The sentence may be reworded.
pub const ERRORS: &[(&str, &str)] = &[
    // References (phase 1).
    (
        "no_record_matches",
        "no node matches `{ref}`; search with `ev find` and retry with an id",
    ),
    ("no_record_with_id", "no node with id {id}"),
    (
        "ref_matches_several",
        "`{ref}` matches {count} nodes; retry with an id",
    ),
    (
        "record_gone",
        "node {id} is gone; `ev show {id} --include-gone` or `ev history {id}` still find it",
    ),
    (
        "record_joined",
        "node {id} joined #{into}; it is counted there now",
    ),
    // Refusals of the records and their places (phase 2).
    (
        "split_holds_things",
        "{node} holds {inside} thing(s); split what is inside, move it out first, or take empty units off it with --take",
    ),
    (
        "split_takes_all",
        "the parts take {take} of the {had} of {node}; nothing would be left on it: keep one part as the original with --rename and --qty instead",
    ),
    (
        "already_candidate",
        "{node} is already a candidate; use `ev gone` or `ev restore`",
    ),
    (
        "restore_gone_by_correction",
        "node {id} is gone; `ev restore {id} --correction \"why\"` undoes it",
    ),
    ("not_a_candidate", "{node} is not a candidate"),
    (
        "gone_needs_how",
        "{node} is active; say how it left with --as trash|give|sell|trade|used|digitize|left|stolen|unknown",
    ),
    ("already_lost", "{node} is already lost"),
    ("home_cannot_be_lost", "a home cannot be lost"),
    (
        "vehicle_cannot_be_lost",
        "a vehicle cannot be lost; a stolen one is gone (--as stolen)",
    ),
    (
        "coverage_edit_nothing",
        "say what to correct: --for, --off, or issuer=, number=, term=, ends=, premium=, deductible=, currency=, scope=, note=",
    ),
    ("coverage_edit_field_bad", "`{field}` is not field=value"),
    (
        "coverage_edit_field_unknown",
        "`{field}` cannot be corrected; use one of {fields}",
    ),
    (
        "coverage_on_nothing",
        "coverage {id} would be on nothing; remove it with ev cover remove if it was a mistake",
    ),
    (
        "purchase_edit_not_manual",
        "purchase {id} came from {source}; correct it there and import it again",
    ),
    (
        "purchase_edit_nothing",
        "say what to correct: name=, date=, paid=, currency=, shop=, brand=, order= or qty=",
    ),
    ("purchase_edit_field_bad", "`{field}` is not field=value"),
    (
        "purchase_edit_field_unknown",
        "`{field}` cannot be corrected; use one of {fields}",
    ),
    (
        "purchase_qty_below_linked",
        "purchase {id} has {linked} unit(s) linked to things; qty {qty} is fewer",
    ),
    (
        "place_is_a_household",
        "{place} has {count} errand(s): it is another household, not a home of ours",
    ),
    (
        "home_leaves_moved_or_sold",
        "a home leaves moved out of (--as moved), sold (--as sell) or as a mistake, not as {way}",
    ),
    (
        "moved_is_a_homes",
        "--as moved is a home's; a {kind} that stayed behind at a move is --as left",
    ),
    (
        "leaving_still_holds",
        "{node} still holds {count} record(s) besides its rooms or compartments; move each where it goes, or say it stayed behind (--as left), first",
    ),
    (
        "vehicle_inside",
        "a vehicle stands at the top beside the homes, never inside another node",
    ),
    ("not_lost", "{node} is not lost"),
    (
        "never_seen",
        "{node} was never seen anywhere; say where it turned up with `ev found <ref> --in <place>`",
    ),
    ("no_pending_move", "{node} has no pending move"),
    (
        "already_there",
        "{node} is already in {place}; a place inside it (a compartment) is a grid cell (`ev grid`, `ev cell`) or a holder of its own",
    ),
    (
        "move_already_pending",
        "{node} already has a pending move; cancel it first",
    ),
    (
        "code_only_digits",
        "code `{code}` is only digits and would read as an id",
    ),
    ("code_in_use", "code `{code}` is already in use"),
    (
        "home_inside_another",
        "a home cannot be placed inside another node",
    ),
    (
        "needs_a_place",
        "a {kind} needs a place: give --in, or --lost if its place is unknown",
    ),
    ("holder_gone", "{node} is gone and cannot hold anything"),
    (
        "room_inside_wrong_kind",
        "a room can only be inside a home or another room, not a {kind}",
    ),
    (
        "into_itself",
        "a node cannot be moved into itself or into something it contains",
    ),
    ("address_only_home", "only a home has an address"),
    (
        "still_holds",
        "{node} still holds {count} active node(s); move or dispose of them first",
    ),
    (
        "digitize_needs_copy",
        "{node} has no copy yet; attach one with `ev photo add` or `ev doc add --for` before it leaves as digitized",
    ),
    ("not_gone", "{node} is not gone"),
    ("left_with", "{node} left with {holder}; restore that first"),
    // Refusals of the purchases.
    (
        "purchase_dismissed",
        "purchase {id} is dismissed as {as}; clear that first",
    ),
    (
        "purchase_not_enough_open",
        "purchase {id} has {open} left to link, not {qty}",
    ),
    ("purchase_join_itself", "purchase {id} cannot join itself"),
    (
        "purchase_join_to_joined",
        "purchase {into} joins {kept} already; join to {kept}",
    ),
    (
        "purchase_join_linked",
        "purchase {id} is linked to a thing; unlink it first, or join the other line to it",
    ),
    (
        "purchase_join_kept",
        "other lines join purchase {id}; join them to the other line instead",
    ),
    (
        "purchase_never_a_thing",
        "purchase {id} is a {bucket} purchase; it is never a thing in the home",
    ),
    (
        "purchase_nothing_open",
        "purchase {id} has nothing left to link",
    ),
    (
        "purchase_already_bucket",
        "purchase {id} is already {bucket}",
    ),
    (
        "purchase_linked_unlink_first",
        "purchase {id} is linked to a thing; `ev buy unlink` it first",
    ),
    (
        "purchase_not_linked_to",
        "purchase {id} is not linked to node {node}",
    ),
    (
        "line_linked_unlink_first",
        "line {id} is linked to #{node}; `ev buy unlink {id} {node}` first",
    ),
    (
        "line_not_declined",
        "line {id} was not declined for #{node}",
    ),
    (
        "left_holds_no_purchase",
        "#{node} left as {how}; it holds no purchase",
    ),
    (
        "bought_after_left",
        "the purchase was bought {bought}, after #{node} left ({left})",
    ),
    // Refusals of the grids.
    (
        "grid_none",
        "this place has no grid; set one with `ev grid <ref> --cols N --rows M`",
    ),
    (
        "cells_outside_grid",
        "{cells} is outside the {cols}×{rows} grid",
    ),
    (
        "grid_too_small",
        "{count} placed box(es) would fall outside a {cols}×{rows} grid",
    ),
    (
        "face_needs_grid",
        "it has no grid; give it one with --cols and --rows",
    ),
    (
        "grid_has_boxes",
        "{count} box(es) are placed in this grid; clear their cells first",
    ),
    ("not_inside_anything", "{node} is not inside anything"),
    (
        "holder_has_no_grid",
        "{holder} has no grid; set one with `ev grid <holder> --cols N --rows M`",
    ),
    (
        "cells_do_not_fit",
        "{cells} does not fit in a {cols}×{rows} grid (columns A–{last_col}, rows 1–{rows})",
    ),
    (
        "cells_shared",
        "{a} ({a_cells}) and {b} ({b_cells}) would share cells",
    ),
    // Refusals of things kept in several places.
    (
        "spread_items_only",
        "{node}: only items are kept in several places",
    ),
    (
        "spread_serial_one_unit",
        "{node}: a record with a serial is one unit",
    ),
    ("spread_lost", "{node}: it is lost; find it first"),
    ("spread_lent", "{node}: it is lent out; take it back first"),
    (
        "spread_pending",
        "{node}: it already has a pending move; cancel it first",
    ),
    (
        "spread_holds_things",
        "{node}: it holds things; move what is inside first",
    ),
    (
        "not_that_many",
        "{node} has {have}; there are not {qty} to take",
    ),
    (
        "portion_not_an_item",
        "{node} is a {kind}, and only items are kept in several places; to make one record of several boxes into two, take some off with `ev split <box> <name>=<n> --take`",
    ),
    (
        "portion_has_serial",
        "{node} has a serial number: it is one unit, not kept in several places",
    ),
    (
        "join_all_gone",
        "every one of them is gone; join needs one that is still here",
    ),
    (
        "join_differ",
        "they differ in {field} ({seen}); set one {field} on all of them first if they are one thing",
    ),
    (
        "not_in_several_places",
        "{node} is not kept in several places",
    ),
    (
        "portion_field_apart",
        "{node} is one portion of a thing kept in several places; its {field} would set it apart: `ev unjoin` it first",
    ),
    // Refusals of the plan: counting and tasks.
    (
        "review_unchanged",
        "#{id} is already {status}; nothing changed since",
    ),
    (
        "task_done_reopen_first",
        "task {id} is done; `ev task reopen {id}` it first if it was not",
    ),
    (
        "task_already",
        "task {id} is already {status}; nothing to change",
    ),
    (
        "task_not_closed",
        "task {id} is not closed; nothing to reopen",
    ),
    (
        "task_dropped_reopen_first",
        "task {id} was dropped; `ev task reopen {id}` it first if it was done",
    ),
    (
        "task_closed_reopen_first",
        "task {id} is closed; `ev task reopen {id}` it first",
    ),
    (
        "task_closed_no_move",
        "task {id} is closed; reopen it before moving it",
    ),
    // Refusals of the marks.
    (
        "no_photo_to_call_current",
        "node {id} has no photo to call current; `ev photo add {id} <file>`",
    ),
    (
        "no_code_no_label",
        "node {id} has no code, so there is no label to print",
    ),
    (
        "empty_not_a_box",
        "{ref} is a {kind}, not a box: only a container is said to be empty",
    ),
    (
        "empty_has_records",
        "{ref} has {count} record(s) in it; move them out first if it is empty",
    ),
    ("not_broken", "node {id} is not marked broken"),
    (
        "not_for_sale",
        "node {id} is not set aside to sell; `ev dispose {id} --as sell` first",
    ),
    ("need_closed", "need {id} is already closed"),
    // Refusals of edits.
    (
        "gone_fields_only",
        "node {id} is gone; only {fields} can change, not `{given}`",
    ),
    (
        "address_clear_first",
        "only a home has an address; clear it first",
    ),
    (
        "holds_rooms",
        "{node} holds rooms, so it must stay a home or a room",
    ),
    (
        "has_not_left",
        "node {id} has not left; `ev gone` says when",
    ),
    (
        "waits_for_itself",
        "{node} cannot wait for itself or something inside it",
    ),
    (
        "not_ours_to_lend",
        "{node} is not ours; it cannot be lent out",
    ),
    // Refusals of the places outside the home.
    ("place_name_taken", "`{name}` already names a place"),
    (
        "places_already_one",
        "both names already point to the same place",
    ),
    ("already_lent_to", "{node} is already lent to {to}"),
    ("not_lent_out", "{node} is not lent out"),
    (
        "alias_names_another",
        "`{alias}` already names another place; use `ev place merge`",
    ),
    // Refusals of the map.
    (
        "beside_needs_size",
        "give its --size first, to place it beside another",
    ),
    (
        "beside_other_place",
        "it can only be placed beside something in the same place",
    ),
    (
        "beside_unplaced",
        "the other has no place yet; sketch it first",
    ),
    (
        "stands_on_itself",
        "a thing cannot stand on itself or on what stands on it",
    ),
    (
        "outside_holder",
        "at {x},{y} and {w}×{d} cm it would lie outside its holder, {pw}×{pd} cm",
    ),
    // Refusals of coverages, the past, kits, photos, documents and placement.
    (
        "coverage_no_line_to_clear",
        "coverage {id} has no purchase line to clear",
    ),
    (
        "coverage_line_dismissed",
        "line {line} is dismissed ({as}); `ev buy dismiss {line} --clear` it first",
    ),
    (
        "coverage_line_linked",
        "line {line} is linked to a thing; `ev buy unlink` it first if it is the coverage's",
    ),
    (
        "coverage_line_taken",
        "line {line} is already coverage {coverage}'s",
    ),
    (
        "not_sold",
        "node {id} did not leave as sold; `ev gone {id} --as sell` first (or `ev dispose {id} --as sell` while it is still here)",
    ),
    (
        "trade_not_left",
        "node {id} has not left; `ev gone {id} --as trade` first",
    ),
    (
        "trade_wrong_leaving",
        "node {id} left as {how}; only a thing given, sold or traded can be a trade",
    ),
    (
        "already_traded_for",
        "node {id} is already recorded as traded for #{for}; nothing to change",
    ),
    (
        "already_traded",
        "node {id} is already recorded as traded; nothing to change",
    ),
    ("kit_name_taken", "there is already a kit named `{name}`"),
    (
        "kit_part_linked",
        "part {part} ({text}) has {count} record(s) linked; `ev kit unlink` them first",
    ),
    (
        "kit_part_not_linked",
        "#{node} is not linked to part {part} of {kit}",
    ),
    (
        "photo_whole_elsewhere",
        "this photo is already attached whole to {count} other node(s); attach a --crop of the part that shows node {id}, or pass --whole if the whole view is meant",
    ),
    (
        "photo_whole_elsewhere_batch",
        "this photo is already attached whole to {count} other node(s)",
    ),
    (
        "mark_needs_grid_photo",
        "no photo of it kept its grid corners; give --grid, or cut the next one with --grid",
    ),
    ("mark_needs_whole_photo", "it has no whole photo to mark"),
    (
        "document_not_linked",
        "document {id} is not linked to node {node}",
    ),
    (
        "no_decline_to_clear",
        "node {id} has no move declined to take back",
    ),
    (
        "bring_not_linked",
        "purchase {id} is not linked to {ref}; ev buy link it first",
    ),
    (
        "layout_too_few_units",
        "{ref} has {count} place(s) gone through on its own; a layout compares two or more",
    ),
    // Usage errors and not found (phase 3).
    (
        "batch_key_outside_batch",
        "`@key` references only work inside a batch",
    ),
    ("batch_key_unknown", "unknown batch key `{key}`"),
    ("batch_key_duplicate", "duplicate batch key `{key}`"),
    (
        "search_text_empty",
        "search text is empty; give text, or --tag / --kind / --empty to list",
    ),
    ("edit_no_lines", "no lines to edit"),
    ("edit_nothing_to_set", "nothing to set"),
    (
        "split_no_parts",
        "give at least one <name>=<qty> to split off",
    ),
    ("split_part_needs_name", "a split-off part needs a name"),
    (
        "upkeep_kind_unknown",
        "`{kind}` is no kind of upkeep: one of {kinds}",
    ),
    ("upkeep_work_empty", "say what was done (--work)"),
    (
        "purchase_about_needs_service",
        "line {id} is {bucket}: a thing's own purchase is linked (ev buy link), only a service or a download is about one",
    ),
    (
        "purchase_about_needs_thing",
        "name the record the line is about, or --clear",
    ),
    (
        "upkeep_due_needs_when",
        "say when it is first due: --next <date>, --next-km n, or both",
    ),
    ("upkeep_at_empty", "say when it was done"),
    (
        "upkeep_next_bad",
        "a due date is YYYY-MM-DD or YYYY-MM, got `{date}`",
    ),
    (
        "upkeep_km_bad",
        "an odometer reading is a whole number of km, got `{km}`",
    ),
    ("upkeep_doc_unknown", "no document with id {doc}"),
    ("upkeep_not_found", "no upkeep with id {id}"),
    ("upkeep_edit_nothing", "give at least one field=value"),
    ("upkeep_edit_field_bad", "`{field}` is not field=value"),
    (
        "upkeep_edit_field_unknown",
        "`{field}` is no field of upkeep: kind, work, at, km, by, next_at, next_km, doc or note",
    ),
    (
        "unobserve_nothing",
        "give an observation id, or --on a place to remove all of its observations",
    ),
    (
        "guess_nothing",
        "name at least one record to mark as a guess",
    ),
    (
        "guess_field_unknown",
        "`{field}` is no field that can be a guess: one of {fields}, or cover:<id>",
    ),
    (
        "guess_cover_not_its",
        "coverage {cover} is not one of #{id}'s coverages",
    ),
    (
        "split_part_moves_and_leaves",
        "the part `{name}` is given both a place to go and a way to leave: one of them",
    ),
    (
        "split_take_with_qty",
        "--take sets the original's count itself; leave --qty out",
    ),
    (
        "split_take_needs_counts",
        "--take needs a count on the original and on every part",
    ),
    ("codes_none_given", "give at least one <ref>=<code>"),
    ("node_given_twice", "{node} is given twice"),
    ("code_given_twice", "code `{code}` is given twice"),
    ("join_needs_two", "name at least two records to join"),
    (
        "nothing_to_use_up",
        "nothing waits to be used up; when it is, record it with `ev gone --as used`",
    ),
    (
        "nothing_set_aside",
        "nothing is set aside to be {way}; record it with `ev gone --as {way}`",
    ),
    (
        "mistake_not_set_aside",
        "a mistaken record is not set aside; close it with `ev gone --as mistake --why`",
    ),
    (
        "mistake_needs_why",
        "say why the record was a mistake with --why",
    ),
    ("reference_empty", "empty reference"),
    (
        "code_series_malformed",
        "`{code}`: a series is a prefix followed by one `*`, like GF1x1-*",
    ),
    ("code_empty", "code is empty"),
    ("qty_below_one", "qty must be at least 1"),
    (
        "purchase_qty_not_a_count",
        "qty {qty} is not a whole count; send a number such as 2",
    ),
    ("fill_out_of_range", "fill must be between 0 and 100"),
    ("tag_empty", "tag is empty"),
    ("photo_path_empty", "photo path is empty"),
    ("photo_path_invalid", "photo path `{path}`: {error}"),
    (
        "past_with_of",
        "a past thing is added on its own: `gone` and `of` do not go together",
    ),
    (
        "leaving_details_without_gone",
        "--at and --where say how a thing left: add it with --gone",
    ),
    ("name_empty", "name is empty"),
    (
        "leaving_way_unknown",
        "`{way}` is no way of leaving; use sell, give, trash, used, trade, return, left, stolen or unknown",
    ),
    (
        "past_way_not_allowed",
        "a past thing is not added as {way}: give how it left (sell, give, trash, used, trade, return, left, stolen or unknown)",
    ),
    (
        "past_in_a_place",
        "a past thing is added on its own: no --in, --lost, home or room",
    ),
    (
        "past_no_place_fields",
        "a past thing has no --to, --temporary or --code: it is no longer here",
    ),
    (
        "past_where_is_a_place",
        "--where names a place (a former home), not a record",
    ),
    (
        "traded_for_without_trade",
        "traded_for goes with a trade: `--gone trade`",
    ),
    ("say_where_with_in", "say where they are with --in"),
    (
        "shred_not_for_way",
        "--shred is for what goes in the bin (trash, digitize), not `{way}`",
    ),
    (
        "merged_not_a_way",
        "`merged` is not a way to leave: ev sets it when a portion joins another",
    ),
    ("restore_needs_why", "say why the node was not really gone"),
    (
        "purchase_not_an_amount",
        "`{amount}` is not an amount like 1234.56",
    ),
    ("not_a_date", "`{date}` is not a date YYYY-MM-DD"),
    ("no_purchase_with_id", "no purchase with id {id}"),
    ("field_required", "`{field}` is required"),
    (
        "purchase_status_unknown",
        "status `{status}`; use delivered, returned or cancelled",
    ),
    (
        "purchase_import_bucket_unknown",
        "bucket `{bucket}`; use {buckets} (an adapter's consumable lines are left out)",
    ),
    ("line_not_json", "not JSON: {error}"),
    ("purchase_line_type_unknown", "unknown line type `{type}`"),
    ("date_still_to_come", "`{date}` is still to come"),
    (
        "purchase_consumable_not_recorded",
        "consumables are not recorded as purchases; use {buckets}",
    ),
    (
        "purchase_manual_cancelled",
        "a manual purchase cannot be cancelled",
    ),
    (
        "purchase_bucket_unknown",
        "bucket `{bucket}`; use {buckets}",
    ),
    (
        "purchase_reason_unknown",
        "`{reason}` is not a reason; use {reasons}",
    ),
    ("purchase_pack_below_one", "pack must be at least 1"),
    (
        "purchase_pack_too_small",
        "purchase {id} has {linked} units linked; a pack of {pack} leaves {units}",
    ),
    (
        "purchase_link_to_place",
        "#{node} is a place; a purchase is linked to a thing",
    ),
    (
        "money_currency_unknown",
        "`{currency}` is no currency code like EUR, USD or TRY",
    ),
    ("money_not_positive", "`{field}` must be a positive number"),
    (
        "money_period_malformed",
        "period `{period}`: YYYY-MM or YYYY",
    ),
    ("money_day_malformed", "day `{day}`: YYYY-MM-DD"),
    (
        "money_line_type_unknown",
        "line type {type}; use index or rate",
    ),
    ("not_web_address", "`{url}` is not a web address"),
    (
        "attachment_link_kind_unknown",
        "`{kind}` is not a link kind",
    ),
    (
        "attachment_coverage_kind_unknown",
        "`{kind}` is not a coverage kind",
    ),
    ("attachment_unknown", "unknown attachment `{attachment}`"),
    (
        "attachment_not_carried",
        "purchase {id} carries no attachment {attachments}; ev buy show {id} lists them",
    ),
    (
        "attachment_type_unknown",
        "attachment type '{type}' is not one of: {types}",
    ),
    ("valuation_not_positive", "a value must be more than zero"),
    ("valuation_no_such_id", "no value with id {id}"),
    (
        "link_kind_unknown",
        "`{kind}` is not a link kind; use one of {kinds}",
    ),
    (
        "link_archive_neither",
        "archive `{archive}`: neither a web address nor a file",
    ),
    ("link_archive_unreadable", "{archive}: {error}"),
    ("link_no_such_id", "link {id}"),
    (
        "doc_kind_unknown",
        "`{kind}` is not a document kind; use one of {kinds}",
    ),
    (
        "doc_date_malformed",
        "`{date}` is not a date; use YYYY-MM-DD, YYYY-MM or YYYY",
    ),
    ("doc_no_such_id", "no document with id {id}"),
    ("doc_no_such_file", "{file}: no such file"),
    ("doc_file_unreadable", "{file}: {error}"),
    (
        "coverage_bad_term",
        "term `{term}`; use e.g. 2y, 18m, 6w, 90d or lifetime",
    ),
    ("coverage_not_found", "no coverage with id {id}"),
    (
        "coverage_setting_unknown",
        "`{setting}` is not an inventory setting; use {settings}",
    ),
    (
        "coverage_setting_bad_value",
        "`{value}` is not a value for {setting}",
    ),
    (
        "coverage_covers_nothing",
        "name at least one thing it covers",
    ),
    (
        "coverage_kind_unknown",
        "`{kind}` is not a coverage kind; use {kinds}",
    ),
    ("coverage_bad_after", "`{after}`: after:<coverage id>"),
    (
        "coverage_needs_term",
        "give a --term (2y, 18m, lifetime) or an --ends date",
    ),
    (
        "coverage_insurance_lifetime",
        "an insurance runs out: give an --ends date or a term in years, months, weeks or days, not lifetime",
    ),
    (
        "coverage_track_unknown",
        "`{subject}` is not tracked; use value or coverage",
    ),
    (
        "coverage_track_decision",
        "`{decision}`; use no, later or yes",
    ),
    ("task_not_found", "no task with id {id}"),
    ("task_bad_due", "due is YYYY-MM-DD, got `{due}`"),
    ("plan_field_empty", "{field} is empty"),
    ("plan_goal_unknown", "goal must be one of {goals}"),
    ("node_has_no_photo", "node {id} has no photo {n}"),
    (
        "plan_series_has_no",
        "the marked photo series has no f{n} ({count} picture(s))",
    ),
    ("plan_names_no_picture", "name at least one picture"),
    ("no_such_file", "no file {file}"),
    ("plan_no_observation", "no observation {id}"),
    (
        "review_thing_not_place",
        "#{id} is a thing, not a place; review the place it is in",
    ),
    (
        "review_status_unknown",
        "review must be counting, toured, kept or raw",
    ),
    (
        "progress_thing_not_place",
        "#{id} is a thing, not a place: give the furniture or room it is in",
    ),
    ("task_status_unknown", "status must be one of {states}"),
    ("mark_bad_use_by", "`{date}` is not YYYY-MM-DD or YYYY-MM"),
    ("need_not_found", "no need with id {id}"),
    ("mark_empty_names_none", "name the boxes that are empty"),
    (
        "mark_condition_unknown",
        "`{condition}` is not a condition; use {conditions}",
    ),
    (
        "mark_sale_state_unknown",
        "`{state}` is not a sale state; use listed or reserved",
    ),
    ("need_text_empty", "need text is empty"),
    ("kit_not_found", "no kit `{kit}`; `ev kit list` shows them"),
    ("kit_part_needs_name", "a kit part needs a name"),
    (
        "kit_part_qty_too_small",
        "`{part}`: a part comes at least once",
    ),
    (
        "kit_part_missing",
        "part {n} does not exist; the kit has {count} parts (`ev kit show` lists them)",
    ),
    (
        "kit_part_missing_numbered",
        "part {n} does not exist; the kit has {count} parts, numbered up to {last} (`ev kit show` lists them)",
    ),
    ("kit_needs_name", "a kit needs a name"),
    ("kit_copies_too_few", "a kit is bought at least once"),
    ("kit_needs_parts", "give at least one part"),
    (
        "kit_link_needs_records",
        "give the records that are this part",
    ),
    ("portion_qty_too_small", "--qty must be at least 1"),
    (
        "sketch_points_bad",
        "--points is three or more corners in centimetres, like `0,0 400,0 400,300`; got `{points}`",
    ),
    (
        "sketch_pair_bad",
        "{what} is two numbers of centimetres, like 120,40; got `{value}`",
    ),
    (
        "sketch_nothing_given",
        "give --size w,d, --at x,y, --points, --on <ref>, --right-of/--left-of/--above/--below <ref> or --clear",
    ),
    ("sketch_clear_alone", "--clear takes nothing else"),
    (
        "sketch_place_once",
        "a place is given once: --at, --points, or beside one other thing",
    ),
    (
        "sketch_outline_too_few",
        "an outline is three or more corners",
    ),
    (
        "sketch_outline_or_size",
        "an outline has its own size; give --points or --size",
    ),
    (
        "sketch_size_bad",
        "a size is two positive numbers of centimetres",
    ),
    (
        "sketch_size_has_outline",
        "it has an outline, which gives its size; give new --points instead",
    ),
    ("sketch_at_bad", "a place is two numbers of centimetres"),
    (
        "sketch_offset_alone",
        "--offset goes with --right-of, --left-of, --above or --below",
    ),
    (
        "map_no_home",
        "no home yet; add one with `ev add <name> --kind home`",
    ),
    ("map_place_gone", "#{id} is gone"),
    (
        "past_date_bad",
        "`{date}` is no date: give a year (2016), a month (2016-06) or a day (2016-06-14)",
    ),
    (
        "past_came_after_left",
        "it came {came} but left {left}; one of the dates is not right",
    ),
    (
        "past_where_not_place",
        "--where names a place (a former home) by its name, not a record or an id",
    ),
    ("past_sale_price_zero", "a sale brought more than nothing"),
    ("trade_for_itself", "a thing is not traded for itself"),
    (
        "trade_for_place",
        "#{other} is a place; a thing is traded for a thing",
    ),
    (
        "trade_other_left_before",
        "#{other} left {other_left}, before the swap ({left}); it cannot have come in exchange",
    ),
    (
        "trade_other_came_before",
        "#{other} came {came}, before the swap ({left}); it was ours already",
    ),
    (
        "trade_other_came_years_after",
        "#{other} came {came}, years after the swap ({left}); one of the dates is not right",
    ),
    ("past_year_bad", "`{year}` is no year to look back on"),
    (
        "photo_not_an_image",
        "{file} is no image ev can read (a JPEG or PNG photo)",
    ),
    (
        "photo_cut_nothing",
        "give at least one <ref>=x,y,w,h, or --place <ref>",
    ),
    (
        "photo_grid_needs_place",
        "--grid reads the boxes of the --place grid; give --place too",
    ),
    (
        "photo_preview_nothing",
        "nothing to preview: give <ref>=x,y,w,h, or --place with --grid",
    ),
    (
        "photo_mark_nothing",
        "give at least one <label>=x,y,w,h or <label>=<cell>",
    ),
    (
        "series_ref_bad",
        "`{text}` is no series picture: give f12, or a range like f16..f31",
    ),
    ("series_is_empty", "the marked photo series has no pictures"),
    (
        "photo_mark_cell_on_file",
        "`{spec}` is a cell: mark a place with a grid (by its code), not a file",
    ),
    (
        "photo_number_missing",
        "photo {n} does not exist; it has {count}",
    ),
    (
        "photo_crop_not_numbers",
        "crop `{crop}` is not four numbers x,y,w,h",
    ),
    (
        "photo_crop_not_four",
        "crop `{crop}` needs exactly four numbers x,y,w,h",
    ),
    (
        "photo_crop_outside",
        "crop `{crop}` must lie inside the photo: fractions 0–1 with x+w ≤ 1 and y+h ≤ 1",
    ),
    (
        "photo_turn_bad",
        "turn a photo by 90, 180 or 270 degrees clockwise, not {degrees}",
    ),
    (
        "edit_size_bad",
        "size is WxDxH or WxD, like 1x2x0.5; got `{size}`",
    ),
    (
        "edit_not_integer",
        "{field} must be an integer, got `{value}`",
    ),
    ("edit_not_assignment", "`{assignment}` is not field=value"),
    ("edit_name_empty", "name cannot be empty"),
    ("edit_note_add_empty", "note=+ needs the text to add"),
    (
        "edit_not_boolean",
        "{field} takes true or false, got `{value}`",
    ),
    (
        "edit_left_in_not_place",
        "left_in names a place (a former home) by its name, not a record or an id",
    ),
    (
        "edit_field_unknown",
        "unknown or read-only field `{field}`; editable: name, code, kind, address, qty, note, theme, fill, size, tags, photos, to, owner, with, temporary, waits_for, make, model, serial, came, and on a gone record left and left_in (how far a place is counted is `ev review`)",
    ),
    (
        "edit_not_plus_minus",
        "{field} takes +value or -value, got `{value}`",
    ),
    ("cell_bad", "`{cell}` is not a cell like A3"),
    (
        "grid_corners_bad",
        "grid corners `{corners}` are eight fractions 0–1: back-left x,y, back-right x,y, front-right x,y, front-left x,y",
    ),
    (
        "grid_size_bad",
        "a grid is 1–{max_cols} columns and 1–{max_rows} rows",
    ),
    (
        "grid_face_bad",
        "a grid is seen from `above` or from the `front`; got `{face}`",
    ),
    ("cell_nothing_given", "give at least one <ref>=<cells>"),
    ("cell_ref_twice", "`{ref}` is given twice"),
    ("placement_no_live_node", "no live node {ref}"),
    (
        "placement_nothing_described",
        "describe the thing to place (a word of 3+ letters or a part code), or give --for",
    ),
    (
        "placement_no_holder_to_stay",
        "it is in no holder to stay in",
    ),
    (
        "vocab_facet_name_bad",
        "a facet needs a name with a searchable word (3+ letters)",
    ),
    ("vocab_no_facet", "no facet {name}"),
    (
        "vocab_synonyms_too_few",
        "give two or more comma-separated words or phrases, each with a searchable word",
    ),
    ("vocab_no_synonym_group", "no synonym group {id}"),
    ("place_name_empty", "place name is empty"),
    (
        "place_no_such",
        "no place named `{name}`; `ev place list` shows them",
    ),
    ("audit_rule_empty", "rule text is empty"),
    ("audit_no_rule", "no rule with id {id}"),
    ("label_required", "{what} is required"),
    (
        "label_unknown",
        "unknown {what} `{value}`; expected one of: {choices}",
    ),
];

/// The English template of an id.
pub fn template(id: &str) -> Option<&'static str> {
    ERRORS.iter().find(|(i, _)| *i == id).map(|(_, t)| *t)
}

/// The names a template fills: `{ref}` gives `ref`.
pub fn placeholders(template: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        let Some(len) = rest[start + 1..].find('}') else {
            break;
        };
        let name = &rest[start + 1..start + 1 + len];
        if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            out.push(name);
        }
        rest = &rest[start + 1 + len..];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{ERRORS, placeholders, template};

    /// Every Rust file of core but the two that define errors.
    fn sources() -> Vec<(String, String)> {
        let mut out = Vec::new();
        let mut dirs = vec![std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src")];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(dir).unwrap().flatten() {
                let path = entry.path();
                if path.is_dir() {
                    dirs.push(path);
                } else if path.extension().is_some_and(|e| e == "rs")
                    && !path.ends_with("error.rs")
                    && !path.ends_with("errors.rs")
                {
                    let text = std::fs::read_to_string(&path).unwrap();
                    out.push((path.display().to_string(), text));
                }
            }
        }
        out
    }

    #[test]
    fn every_id_is_written_once_in_snake_case() {
        let mut ids: Vec<&str> = ERRORS.iter().map(|(i, _)| *i).collect();
        assert!(
            ids.iter()
                .all(|i| i.chars().all(|c| c.is_ascii_lowercase() || c == '_'))
        );
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), before, "an id is written twice");
    }

    #[test]
    fn every_id_raised_has_a_sentence() {
        for (path, text) in sources() {
            for start in [
                "Error::said(",
                "refuse(\n",
                "refuse(\"",
                "usage(\n",
                "usage(\"",
                "not_found(\n",
                "not_found(\"",
            ] {
                for part in text.split(start).skip(1) {
                    // `refuse(\n    "id"`, or `refuse("id"`; an id passed as a variable is
                    // checked where it is written.
                    let id = if start.ends_with('"') {
                        part.split('"').next()
                    } else if part.trim_start().starts_with('"') || start == "Error::said(" {
                        part.split('"').nth(1)
                    } else {
                        None
                    };
                    let Some(id) = id else { continue };
                    assert!(template(id).is_some(), "{path}: `{id}` has no sentence");
                }
            }
        }
    }

    #[test]
    fn a_template_names_its_values_plainly() {
        for (id, t) in ERRORS {
            assert!(!placeholders(t).is_empty() || !t.contains('{'), "{id}: {t}");
        }
    }

    /// Errors raised as a bare sentence, without an id, may only become fewer
    /// (spec/error-ids.md); lower the number as they are given ids.
    #[test]
    fn errors_without_an_id_only_become_fewer() {
        const LEFT: usize = 4;
        let n: usize = sources()
            .iter()
            .map(|(_, t)| {
                [
                    "Error::Usage(",
                    "Error::NotFound(",
                    "Error::Ambiguous {",
                    "refused(",
                ]
                .iter()
                .map(|p| t.matches(p).count())
                .sum::<usize>()
            })
            .sum();
        assert!(n <= LEFT, "{n} errors without an id, more than {LEFT}");
    }
}
