"""Answers: Markdown blocks (paragraphs, a heading, code, a list, a table)
read as text; thoughts and tool calls show as one-line rows, folded into a
summary that opens; a long answer streams in."""

from _common import open_project_session, send


def run(h, s):
    open_project_session(h, title="Answers")

    s.step("tool calls fold into a summary")
    send(h, 'tool glob {"pattern":"*.txt"} ;; tool read {"path":"a.txt"} ;; say read them both')
    h.wait_says("read them both", timeout=30)
    summary = h.wait("Searched 1 time, read 1 file", timeout=10)
    h.shot("folded")
    h.click("Searched 1 time, read 1 file")
    h.settle(0.6)
    h.shot("unfolded")

    s.step("a long answer streams in")
    send(h, "think planning the reply | pace 15 | long 3")
    h.wait("Stop", timeout=20)
    h.shot("streaming")
    h.wait_gone("Stop", timeout=60)
    texts = h.texts()
    s.expect(any("Harlo keeps the transcript readable" in t for t in texts), "the prose", texts[:8])
    h.shot("streamed")
