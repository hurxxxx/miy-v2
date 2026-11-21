# miy common UI 2.0.0

Generated from the maintained packages/ui source. Use SDK bundle 0.12.0.
Install through miy component add ui --version 2.0.0; do not edit the
installed component or use platform administrator APIs as a data provider.

This version is the AI-DO UI package, moved as it is: lib/ holds AI-DO's
primitives, layout, overlay, data display and feedback components (Tailwind
CSS 4, Radix UI, lucide-react). The block editor is not included. The miy
composite components remain: DirectoryPicker, ShareEditor, FileTransferPanel
and FileImagePreview, with formatByteSize, requestId and moveItem. The 1.x
Field, Loading and classNames are gone (use Input, ContentState and cn), and
Button, IconButton, Badge, Panel, ContentState, Dialog, useConfirm and
usePrompt are now the AI-DO ones with AI-DO's props.

## npm packages

A component carries source only. The app lists these packages in its
package.json and installs them; miy component sync and miy app check fail with
component_dependencies_missing until it does. React and React DOM come from the
app as before.

- @radix-ui/react-dialog 1.1.15
- @radix-ui/react-dropdown-menu 2.1.16
- @radix-ui/react-scroll-area 1.2.10
- @radix-ui/react-select 2.2.6
- @radix-ui/react-slider 1.4.1
- @radix-ui/react-tabs 1.1.13
- @radix-ui/react-toast 1.2.15
- @radix-ui/react-tooltip 1.2.8
- @tailwindcss/vite 4.2.2
- @tanstack/react-table 8.21.3
- class-variance-authority 0.7.1
- clsx 2.1.1
- lucide-react 0.546.0
- react-is 18.3.1
- recharts 3.8.1
- tailwind-merge 3.5.0
- tailwindcss 4.2.2

## Tailwind

The components are styled with Tailwind utility classes, so the app builds
them. Add the @tailwindcss/vite plugin to the Vite config and give the app one
Tailwind entry CSS that imports this component's theme.css after Tailwind and
scans the installed component sources:

    @import "tailwindcss";
    @import "../../platform-components/ui/theme.css";
    @source ".";
    @source "../../platform-components";

The paths are for an entry CSS in web/src. Without the @source line Tailwind
does not see the component classes and the components render unstyled.
theme.css registers the color and font names the classes use (bg-ui-*,
text-ui-*, border-ui-*, bg-app-*) and the class based dark variant.

Import components from the installed index. It loads styles.css, so the app
does not import that file. The workspace equivalent is @miy/ui.

## Tokens and dark mode

The package declares no design token values. The --ui-* tokens, the Pretendard
font and the theme are supplied by the platform: the frame runtime delivers
them to every app frame and sets the dark class and data-miy-theme="dark" on
the document root. Dark mode follows the document root, not a nested element.

## Data providers

Directory and other data providers return only the app's permitted public DTOs.
Selection and disabled controls do not grant authority: the app server checks
current identity, membership and resource ACL again for each operation.
