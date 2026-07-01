-- The backend already knows html-vs-text when parsing (mail-parser's
-- body_html/body_text), but collapsed both into one `body` column and threw
-- the signal away. The frontend then had to re-guess HTML via a tag regex,
-- which misclassified real HTML mail using only tags like <a>/<img>/<span>
-- (not in the regex) as plain text. Persist the parser's own answer instead.
ALTER TABLE messages ADD COLUMN body_is_html INTEGER NOT NULL DEFAULT 0;
