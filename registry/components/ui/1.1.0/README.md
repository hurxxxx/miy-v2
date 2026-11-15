# miy common UI 1.1.0

Generated from the maintained packages/ui source. Use SDK bundle 0.10.0.
Install through miy component add ui --version 1.1.0; do not edit the
installed component or use platform administrator APIs as a data provider.

Import components from the installed index and include its styles.css once in
the app entrypoint. The workspace equivalent is @miy/ui and @miy/ui/styles.css.
React is supplied by the app bundle; this component installs no npm packages.

Directory and other data providers return only the app's permitted public DTOs.
Selection and disabled controls do not grant authority: the app server checks
current identity, membership and resource ACL again for each operation.
