# Synthetic test inputs only. No real account, endpoint or credential.
GET http://fixture.invalid/search?keep=a%2Bb&empty=
X-Dup: first
X-Dup: second
X-Empty:
X-Literal: apostrophe's $(printf STAYS_LITERAL) `printf ALSO_LITERAL`
[QueryStringParams]
tag: red blue
tag: a+b
word: 日本語
HTTP 200

POST http://fixture.invalid/form
X-Case: form
[FormParams]
item: one two
item: =&@
word: 日本語
HTTP 200

POST http://fixture.invalid/raw
X-Case: raw
```raw
@first's 日本語
literal \n and {{raw}} $(printf RAW_LITERAL) `printf STILL_LITERAL`
```
HTTP 200

POST http://fixture.invalid/text
X-Case: text
```
one\n is literal; next is a real line
two's 日本語
```
HTTP 200

POST http://fixture.invalid/oneline
X-Case: oneline
`one\nescaped's 日本語 \u{7b}{literal}}`
HTTP 200

POST http://fixture.invalid/empty
X-Case: empty
``
HTTP 200

GET http://fixture.invalid/question?
X-Case: question
[Query]
word: a b
HTTP 200

GET http://fixture.invalid/UNSELECTED_SENTINEL
X-Case: omitted
HTTP 200
