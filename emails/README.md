# emails

The emails oryxa sends, written in React Email in the Oryxa website language, dark and light. The
build turns each template into static HTML and plain text with `{{placeholders}}`; the core fills
them in and sends them through SES. Nothing here sends email or holds a code.

```
npm run build     # dist/<name>.html (dark), <name>.light.html, <name>.txt, manifest.json
npm run preview   # preview/ with sample values, to open in a browser
npm test          # builds, then checks placeholders, Outlook, Gmail and plain text
```

- **Pictures** are served from `EMAIL_ASSETS` (default `https://app.oryxa.in/email`), from `assets/`.
- **The core must HTML-escape every value** it puts into a placeholder: names and organisations
  come from people.
- `prototypes/no-picture.tsx` is a design prototype, not part of the build.
