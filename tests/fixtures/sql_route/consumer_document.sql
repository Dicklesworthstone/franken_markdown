-- FCB-009/FCB-011 consumer verification document.
-- Exercises: line comments, block comments, string doubling, quoted
-- identifiers, parameters, numbers across magnitudes, keyword core.
SELECT "col one", id, amount
FROM "my schema"."orders"
WHERE status = 'pending'          -- pending orders only
  AND note LIKE '%it''s%'
  AND amount > 100.50
  AND region = $1
  AND meta @> :filters
  AND created_at BETWEEN '2026-01-01' AND TIMESTAMP '2026-06-30 23:59:59'
  /* multi-line exclusion window:
     archived rows are read through the confined reader */
ORDER BY created_at DESC
LIMIT 50;

UPDATE "my schema"."orders"
SET note = 'reviewed on 2026-06-30 -- verified /* inline */ marker'
WHERE id = $2;

INSERT INTO audit_log (id, query_hash, payload)
VALUES (0x1A, 'a1b2c3', '{ "deltas": [1, 2.5, -3e2], "ok": true }');

SELECT ?, :param, '', 'it''s' FROM dual;
