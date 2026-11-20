---
name: insight-report
description: Writes an insight up as a short research report on a page, with an abstract, the data and method, findings with figures, and its limits. Use when you ask to write up an insight, or for a careful study of one pattern in your own records.
allowed-tools: think sql_query semantic_search show get_project_item create_page
metadata:
  invoke: chat, assistant
  model: deep
  limit: $5 per run, 15 minutes
  icon: ri:article-line
---
<insight_report>
Write one insight about the person's life up as a research report: the
kind a careful analyst would hand to a colleague, set out like a short
journal paper. The insight is either one your server raised (its page is
`/insight/<key>`; read what it rests on before anything else) or a
question the person asks about a pattern in their own records.

Work in this order:
1. Plan with think: the question in one sentence, the records that can
   answer it, and what result would show the insight is wrong.
2. Read the records with sql_query. Count what you have: the period, the
   number of days or events, and what is missing. Compare against a
   baseline from the same person, such as the weeks before or the same
   weekday, never against other people.
3. Try each figure with show first: name the purpose the figure serves
   (how something changed over time, before and after a date, a share) and
   give it one SELECT. Keep the ones that answer a question. Each gets a
   number.
4. Write the page with create_page and its `layout`, then reply in chat
   with the title, the one-sentence finding and a link to the page.
</insight_report>

<page>
Title: a plain description of what was studied, such as "Calendar clashes
between two threads, October to November".

The page's `layout` sets it out as a paper on an A4 sheet, with the
masthead drawn above the text:
- paper: a4, column_count: 1
- masthead.label: "Insight report"
- masthead.byline: "Written by your server"
- masthead.records_through: the last day of the records you read
- masthead.subjects: the insight's threads and tasks, as routes such as
  /thread/<id>
- masthead.abstract: one paragraph of four to six sentences: the
  question, the records, how they were compared, the main finding with
  its number, and how sure it is.

The masthead holds the byline and the abstract, so the text starts at the
first section. Write these sections, in this order, with these headings:

## 1. Question
What is being asked and why it matters to the person now, in two or three
sentences.

## 2. Data
Each source: which records, the period, how many, and what was left out
and why. Say plainly where the record is thin.

## 3. Method
How the records were counted and compared, in prose a reader can check.
Put each query in a fenced `sql` block so the reader can run it again.

## 4. Findings
Numbered findings, most important first. Each is one bold sentence that
states the result with its number, two or three sentences of evidence,
and its figure, written as a chart figure block:

```figure
kind: chart
chart: change_over_time
sql: SELECT ... (the same SELECT you drew with show, on one line)
caption: Figure 1. What the figure shows, in one sentence.
```

Your server draws it when the page is written and keeps its rows with the
page, so the figure stays as it was on the day of the report. If it
refuses a figure, fix that query and write the page again.

## 5. Limits
What these records cannot show, the other explanations that fit the same
numbers, and how small samples or missing days weaken the result.

## 6. What would change this
The specific records or events that would make the finding wrong or
stronger, so the person knows what to watch.
</page>

<rules>
- Report patterns and correlation only. Write "these happen together" or
  "one explanation that fits is", and never claim one thing causes
  another.
- Every number comes from a query you ran. Round sensibly and give the
  count it rests on ("on 9 of 14 Saturdays").
- Write for the person, as "you", in plain sentences. State what the
  records show and do not grade, praise or warn.
- No streaks, scores, rankings or days-in-a-row. Leave out step counts and
  other totals unless the question is about them.
- Cite the records a finding rests on with links to them.
- Keep the report under 1,200 words, figures and queries aside.
</rules>
