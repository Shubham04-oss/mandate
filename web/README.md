# oryxa web app

## Sample data

Every person, organization, email address and team on the screens is made up. They are not
customers or real people, and they exist only so the screens render until the core supplies real
data.

- `src/lib/sample.ts`: Ada Brennan, Raya Patel and Northstar, for the pop-up preview only.
- `src/routes/preview.popups.tsx`: the team Platform, Kenji Sato, `maria@northwind.de`, and the
  "Q3 revenue figures" request. Outside `npm run dev` the page is not found.
- `src/lib/api/standin.ts`: the stand-in API responses (below): the same people and organizations
  (also Harbourline and Tidewater Labs), plus made-up agents, approvals, conversations, grants and
  activity, and the two-step sign-in QR code and key.
- `public/avatars/` and `public/sample/`: the avatar drawings and the QR code image.

None of it reaches a real screen: a production build has no stand-ins, and every screen gets its
data from the core.

## Talking to the core

Every call to the core goes through one module, `src/lib/api/index.ts`, built on the API contract in
section 4 of `docs/superpowers/specs/2026-11-21-v1-remaining-design.md`: it sends
`credentials: "include"`, adds `X-Mandate: 1` to every state-changing request, reads the
`{ "error": { "code", "message" } }` format (with `Retry-After` on 429), and returns the sign-in
outcomes as they are. Screens never call `fetch` themselves.

The base URL comes from `VITE_MANDATE_API_URL`, fixed when the app is built:

- **Unset, in `npm run dev` and tests:** the module answers every call with stand-in responses from
  `src/lib/api/standin.ts`, shaped exactly like the contract. Nothing leaves the browser.
- **Set:** the module calls the core at that address. Nothing else changes; the screens stay as
  they are.
- **A production build** (`npm run build`) fails without it, so a deployed app never shows the
  stand-ins' made-up people; they aren't in a production bundle at all. Deploying is in
  `docs/deploy.md` (the web app).

Locally, both run on `localhost` so the session cookie works across them. Start the core with
`MANDATE_WEB_ORIGIN=http://localhost:3000` and `MANDATE_API_ORIGIN=http://localhost:8080` (plus its
WorkOS settings; see the root README), then:

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

Checked against the core on `main` (`core/src/api`). The contract is unchanged by the web app.

**Still open**

1. **Google sign-in.** The sign-in and sign-up screens have "Continue with Google", but there is no
   Google endpoint, so the buttons are disabled ("Not available yet").
2. **Ways in for `method_required`.** The core passes WorkOS's method names through. The screen
   offers `sso`, `magic_auth`, `password` and (disabled) `google_oauth`; any other way (Microsoft,
   GitHub, Apple, passkeys) gets a note to ask the admin. The outcome also lacks the organization's
   name, which the screen would use ("Northstar asks its members…").
3. **Reset links.** The core maps WorkOS errors to contract codes, but a used or expired reset token
   isn't among them, so unless WorkOS answers 404 it comes back as `internal` (502, "Sign-in is
   unavailable"). The web app treats `code_expired` and `not_found` as an expired link.
4. **Timestamps.** The format isn't stated; the app assumes RFC 3339 strings.
5. **Invitations.** The invitation has no member list, so the faces row on the invite card is gone.
6. **Approval `effect`.** The values aren't listed; the app shows a plain description built from
   the tool name and arguments instead.
7. **Desktop notifications.** Events stream per conversation only. Nothing tells the web app that a
   new approval needs the person outside an open conversation, so Settings → Notifications sends
   only the test.

**Conversations** (`core/src/api/conversations.rs`)

8. **`Last-Event-ID` and CORS.** The browser sends `Last-Event-ID` only when its own EventSource
   reconnects; the core's CORS allows only `content-type` and `x-mandate`. If a browser preflights
   that reconnect, it fails and the app opens a fresh stream, which replays from the start. The app
   drops events it has already seen by id, so nothing shows twice either way. Not yet tried against
   a running core.
9. **`Retry-After` isn't exposed** to the page by CORS, so on 429 the app backs off on its own
   schedule (1 s up to 30 s).
10. **Tool events carry no target.** A step says "Read a web page" but not which page; the approval
    has the arguments, so only steps that wait for an OK show the page.
11. **No `GET /v1/conversations/{id}`.** A conversation's title and state come from the first page of
    the list; one further back opens titled "Conversation", with its state taken from its events.

**Grants** (`core/src/api/grants.rs`)

12. **No tool catalog endpoint.** Settings → Tools names `web.read_page` and `web.submit_form` itself
    (`src/lib/grants.ts`).
13. **`standing` changes nothing in v1.** The policy never runs `external` effects unattended, and
    reads under a grant run anyway, so the app always sends `standing: false` and offers no switch.
14. **Who can be chosen.** People come from the first page of `/v1/org/members`, and agents from
    `/v1/agents`, which lists only the agents the admin may use, not every agent in the
    organization. A grant for anyone else shows their id instead of their name.

**Settled by the core**

- **Local addresses:** the core listens on `127.0.0.1:8080`, which `localhost` reaches, and its
  tests use `http://localhost:3000` as the web origin. The session cookie is `Secure; SameSite=Lax`,
  which browsers accept on `localhost`.
- **Signed out:** HTTP 401 with `forbidden`, which the app treats as signed out.
- **SSO callback:** success lands on `/`; failure on `/sign-in?error=<code>`.
- **Sign-up** takes `org_name` and makes the person the new organization's admin; the screen asks
  for it.
- **Request bodies** for every auth route match the core's.

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
