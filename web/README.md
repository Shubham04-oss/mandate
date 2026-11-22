# oryxa web app

## Sample data

Every person, organization, email address and team on the screens is made up. They are not
customers or real people, and they exist only so the screens render until the core supplies real
data.

- `src/lib/sample.ts`: Ada Brennan (`ada@northstar.io`), Raya Patel, Priya, Rohan, and the
  organizations Northstar, Harbourline and Tidewater Labs. Also the two-step sign-in QR code and
  key.
- `src/routes/preview.popups.tsx`: the team Platform, Kenji Sato, `maria@northwind.de`, and the
  "Q3 revenue figures" request.
- `src/lib/api/standin.ts`: the stand-in API responses (below) reuse the same people and
  organizations, plus made-up agents, approvals and activity.
- `public/avatars/` and `public/sample/`: the avatar drawings and the QR code image.

Replace each with data from the core as the screens are wired; none of it should ship.

## Talking to the core

Every call to the core goes through one module, `src/lib/api/index.ts`, built on the API contract in
section 4 of `docs/superpowers/specs/2026-11-21-v1-remaining-design.md`: it sends
`credentials: "include"`, adds `X-Mandate: 1` to every state-changing request, reads the
`{ "error": { "code", "message" } }` format (with `Retry-After` on 429), and returns the sign-in
outcomes as they are. Screens never call `fetch` themselves.

The base URL comes from `VITE_MANDATE_API_URL`:

- **Unset (the default):** the module answers every call with stand-in responses from
  `src/lib/api/standin.ts`, shaped exactly like the contract. Nothing leaves the browser.
- **Set:** the module calls the core at that address. Nothing else changes; the screens stay as
  they are.

Locally, both run on `localhost` so the session cookie works across them:

```bash
VITE_MANDATE_API_URL=http://localhost:8080 npm run dev
```

The web app is then on `http://localhost:3000` and the core on `http://localhost:8080`.

### Trying the sign-in steps on stand-ins

Any password signs in, except `wrong`. The part of the email before the @ picks the outcome:

| Email starts with | Goes to |
| --- | --- |
| `mfa@` | Enter your code (two-step sign-in) |
| `setup@` | Set up two-step sign-in |
| `orgs@` | Choose an organization |
| `verify@` | Check your inbox |
| `sso@` | Leaves for the organization's sign-in (here, back to sign-in) |
| `method@` | Sign in another way |
| `noaccess@` | No access yet |
| anything else | Home |

The code `000000` is always wrong; any other six digits work. A reset or invitation token of
`expired` has expired; any other token works (`/reset-password?token=x`, `/invite?token=x`). Only
`@northstar.io` addresses have single sign-on.

### Contract notes for the lead

Where the contract and the screens don't line up yet. The contract is unchanged; each note says what
the web app assumes for now.

1. **Local addresses.** 4.1 puts the core on `127.0.0.1:8080` and 4.2 says `localhost` for both. A
   session cookie set by `127.0.0.1` isn't sent from `localhost`, so the web app uses
   `http://localhost:8080`. The core also needs to allow `http://localhost:3000` as an origin with
   credentials.
2. **Google sign-in.** The sign-in and sign-up screens have "Continue with Google", but 4.3 has no
   Google endpoint. The buttons are disabled ("Not available yet"). The `methods` list in
   `method_required` may also name Google.
3. **Signed out.** No error code says "no session". The app treats any HTTP 401 as signed out and
   goes to sign-in; the stand-ins send 401 with `forbidden`.
4. **Timestamps.** The format isn't stated; the app assumes RFC 3339 strings.
5. **`method_required`.** The method names aren't listed; the app assumes `sso`, `magic_code`,
   `password` and `google`. The screen would also like the organization's name ("Northstar asks
   its members…"), which the outcome doesn't carry.
6. **SSO callback errors.** How the core reports a failed SSO sign-in back to the web app isn't
   stated; the app assumes `/sign-in?error=<code>` with a contract error code.
7. **Reset links.** The error for a used or expired reset token isn't stated; the app treats
   `code_expired` and `not_found` as expired. "Link expired" can't resend without the email, so it
   sends the person to "Reset your password" to enter it again.
8. **Invitations.** The invite screen was drawn for someone already signed in; the contract accepts
   while signed out (a code to the invited email, or a name and password), so the screen offers
   both. The invitation has no member list, so the faces row on the card is gone.
9. **Not in the contract yet.** These stay on sample data: "Working now" on Home (tasks in progress,
   the live browser, scheduled runs), the ask box and quick actions (asking an agent from the web),
   the Tools pop-up (connections), and activity entries for access that ended.
10. **Approval `effect`.** The values aren't listed; the app shows a plain description built from
    the tool name and arguments instead.
11. **No event stream.** Settings → Notifications lets a person turn on desktop notifications in
    their browser, but the contract has no way for the core to tell the web app that something new
    needs them (no stream, no polling endpoint for changes). Until there is, only the test
    notification is sent.

## Settings kept on this device

Settings → Appearance (theme, reduce motion, time format) and Settings → Notifications (desktop
notifications) belong to the web app, not the core. They are kept in the browser's local storage
(`mandate.theme`, `mandate.reduce-motion`, `mandate.clock`, `mandate.notify`), so each device keeps
its own and the core never sees them. Theme and reduce motion are applied in the page head before
the first paint.

# Getting Started

To run this application:

```bash
npm install
npm run dev
```

# Building For Production

To build this application for production:

```bash
npm run build
```

## Styling

This project uses [Tailwind CSS](https://tailwindcss.com/) for styling.

### Removing Tailwind CSS

If you prefer not to use Tailwind CSS:

1. Remove the demo pages in `src/routes/demo/`
2. Replace the Tailwind import in `src/styles.css` with your own styles
3. Remove `tailwindcss()` from the plugins array in `vite.config.ts`
4. Remove `@tailwindcss/vite` and `tailwindcss` from `package.json`

## Linting & Formatting

This project uses [Biome](https://biomejs.dev/) for linting and formatting. The following scripts are available:


```bash
npm run lint
npm run format
npm run check
```


## Deploy to Cloudflare Workers

This project uses the Cloudflare Vite plugin (configured in `vite.config.ts`) and `wrangler.jsonc`:

1. Install Wrangler: `npm install -g wrangler`
2. Authenticate: `wrangler login`
3. Deploy: `npx wrangler deploy`

For production env vars, run `wrangler secret put MY_VAR` for each secret listed in `.env.example`. Public (non-secret) vars go in `wrangler.jsonc` under `vars`.

KV, D1, R2, and Durable Object bindings are configured in `wrangler.jsonc` — see https://developers.cloudflare.com/workers/wrangler/configuration/.



## Routing

This project uses [TanStack Router](https://tanstack.com/router) with file-based routing. Routes are managed as files in `src/routes`.

### Adding A Route

To add a new route to your application just add a new file in the `./src/routes` directory.

TanStack will automatically generate the content of the route file for you.

Now that you have two routes you can use a `Link` component to navigate between them.

### Adding Links

To use SPA (Single Page Application) navigation you will need to import the `Link` component from `@tanstack/react-router`.

```tsx
import { Link } from "@tanstack/react-router";
```

Then anywhere in your JSX you can use it like so:

```tsx
<Link to="/about">About</Link>
```

This will create a link that will navigate to the `/about` route.

More information on the `Link` component can be found in the [Link documentation](https://tanstack.com/router/v1/docs/framework/react/api/router/linkComponent).

### Using A Layout

In the File Based Routing setup the layout is located in `src/routes/__root.tsx`. Anything you add to the root route will appear in all the routes. The route content will appear in the JSX where you render `{children}` in the `shellComponent`.

Here is an example layout that includes a header:

```tsx
import { HeadContent, Scripts, createRootRoute } from '@tanstack/react-router'

export const Route = createRootRoute({
  head: () => ({
    meta: [
      { charSet: 'utf-8' },
      { name: 'viewport', content: 'width=device-width, initial-scale=1' },
      { title: 'My App' },
    ],
  }),
  shellComponent: ({ children }) => (
    <html lang="en">
      <head>
        <HeadContent />
      </head>
      <body>
        <header>
          <nav>
            <Link to="/">Home</Link>
            <Link to="/about">About</Link>
          </nav>
        </header>
        {children}
        <Scripts />
      </body>
    </html>
  ),
})
```

More information on layouts can be found in the [Layouts documentation](https://tanstack.com/router/latest/docs/framework/react/guide/routing-concepts#layouts).

## Server Functions

TanStack Start provides server functions that allow you to write server-side code that seamlessly integrates with your client components.

```tsx
import { createServerFn } from '@tanstack/react-start'

const getServerTime = createServerFn({
  method: 'GET',
}).handler(async () => {
  return new Date().toISOString()
})

// Use in a component
function MyComponent() {
  const [time, setTime] = useState('')
  
  useEffect(() => {
    getServerTime().then(setTime)
  }, [])
  
  return <div>Server time: {time}</div>
}
```

## API Routes

You can create API routes by using the `server` property in your route definitions:

```tsx
import { createFileRoute } from '@tanstack/react-router'
import { json } from '@tanstack/react-start'

export const Route = createFileRoute('/api/hello')({
  server: {
    handlers: {
      GET: () => json({ message: 'Hello, World!' }),
    },
  },
})
```

## Data Fetching

There are multiple ways to fetch data in your application. You can use TanStack Query to fetch data from a server. But you can also use the `loader` functionality built into TanStack Router to load the data for a route before it's rendered.

For example:

```tsx
import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/people')({
  loader: async () => {
    const response = await fetch('https://swapi.dev/api/people')
    return response.json()
  },
  component: PeopleComponent,
})

function PeopleComponent() {
  const data = Route.useLoaderData()
  return (
    <ul>
      {data.results.map((person) => (
        <li key={person.name}>{person.name}</li>
      ))}
    </ul>
  )
}
```

Loaders simplify your data fetching logic dramatically. Check out more information in the [Loader documentation](https://tanstack.com/router/latest/docs/framework/react/guide/data-loading#loader-parameters).



# Learn More

You can learn more about all of the offerings from TanStack in the [TanStack documentation](https://tanstack.com).

For TanStack Start specific documentation, visit [TanStack Start](https://tanstack.com/start).
