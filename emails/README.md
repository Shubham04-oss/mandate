# emails

The emails oryxa sends, written in React Email in the Oryxa website language, dark and light. The
build turns each template into static HTML and plain text with `{{placeholders}}`; the core fills
them in and sends them through SES. Nothing here sends email or holds a code.

```
npm run build     # dist/<name>.html (dark), <name>.light.html, <name>.txt, manifest.json
npm run preview   # preview/ with sample values, to open in a browser
npm test          # builds, then checks placeholders, Outlook, Gmail and plain text
```

- **Pictures** live in `web/public/email/`, so the web app's deploy serves them at
  `https://app.oryxa.in/email` (`EMAIL_ASSETS` overrides it); a test checks every one the emails use.
- **Filling placeholders** (names and organisations come from people):
  - `.html` and `.light.html`: HTML-escape every value, including in `href`; `{{url}}` is always a
    link the core made (`https://`).
  - `.txt` and the subject: the raw value, never HTML-escaped, with CR and LF removed.
- **The approval email stays generic** (owner's call, as for in-app alerts): it names who asked and
  which agent, never what the agent wants to do; the details show after sign-in.
