import type { TFunction } from 'i18next';

/**
 * The API names a resource's location with a few fixed English labels when it is
 * not inside a team, list or meeting (see source_access/targets.py and the
 * whiteboard registry). Every other label is a user-chosen name and is shown as is.
 */
const FIXED_LOCATION_LABEL_KEYS: Readonly<Record<string, string>> = {
  'Workspace Docs': 'common:resourceLocation.workspaceDocs',
  'Workspace Whiteboards': 'common:resourceLocation.workspaceWhiteboards',
  Unfiled: 'common:resourceLocation.unfiled',
};

export function localizeResourceLocationLabel(
  label: string | null | undefined,
  t: TFunction,
): string {
  if (!label) return '';
  const key = FIXED_LOCATION_LABEL_KEYS[label];
  return key ? t(key, { defaultValue: label }) : label;
}
