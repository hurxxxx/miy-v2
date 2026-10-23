# Basic miy UI

Import `Button` and `Panel` from `@miy/ui` (the compatibility entry) or
`@miy/components/ui`. `Button` accepts React button attributes; `Panel` accepts
`title: string` and `children: ReactNode`.

```tsx
import { Button, Panel } from "@miy/ui";
<Panel title="My app"><Button onClick={() => runtime.navigate("/detail")}>Open</Button></Panel>
```

No API permission is required. These are presentation components; they do not
implement authentication. Component files are managed by the CLI. Keep business
changes under `web/src`; update a component with `miy app components update ui`.
