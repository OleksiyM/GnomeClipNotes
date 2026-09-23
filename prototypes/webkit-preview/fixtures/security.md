# Preview safety fixture

> A quote with `inline code`, **strong text**, and a [web link](https://example.com/?a=1&b=2).

- [x] completed task
- [ ] open task
  1. nested ordered item
  2. literal characters: & < > " '

| Feature | State |
| --- | --- |
| tables | enabled |
| images | text placeholders |

![remote kitten must not load](https://example.com/tracker.png)

Raw HTML is displayed literally:

<script>alert("never executed")</script>
<img src="file:///etc/passwd" onerror="alert(1)">

Unsafe destinations retain their label but are not clickable:
[javascript](javascript:alert(1)), [data](data:text/html,x), and [file](file:///etc/passwd).
