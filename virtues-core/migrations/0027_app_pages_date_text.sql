-- app_pages.date links a page to a day as a YYYY-MM-DD tag (used by reflections
-- / journal entries). The Rust model treats it as Option<String> and binds it as
-- text everywhere, which fails against a real DATE column with
-- "operator does not exist: date = text". Align the column type with the code.
--
-- date::text renders as YYYY-MM-DD, exactly the format the code reads and writes.
ALTER TABLE app_pages ALTER COLUMN date TYPE text USING date::text;
