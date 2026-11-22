// AI-DO packages/ui/src/lib/editor (BlockNote), reduced to what PMS uses: the
// block editor and viewer with AI-DO's schema (callout, divider, mention,
// task reference). The common UI 2.0.0 left the editor out, so the app
// carries it (miy). The collaborative editor, the mini editor and the
// markdown helpers are not ported.
export { BlockEditor } from './block-editor';
export type { BlockEditorProps } from './block-editor';

export { BlockViewer } from './block-viewer';
export type { BlockViewerProps } from './block-viewer';

export type {
  BlockContent,
  MentionSuggestion,
  TaskRefSuggestion,
} from './types';
