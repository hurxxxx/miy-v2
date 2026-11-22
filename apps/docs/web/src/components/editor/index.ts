// AI-DO packages/ui/src/lib/editor (BlockNote 0.52.1), reduced to what Docs
// uses: the block editor and viewer with AI-DO's schema (callout, divider,
// mention, task reference) and the Markdown codec. The common UI 2.0.0 left
// the editor out, so the app carries it (miy, same as the PMS port). The
// collaborative (Yjs) editor is not ported: real-time co-editing is excluded.
export { BlockEditor } from './block-editor';
export type { BlockEditorProps } from './block-editor';

export { BlockViewer } from './block-viewer';
export type { BlockViewerProps } from './block-viewer';

export { blockContentToMarkdown, markdownToBlockContent } from './markdown';
export type {
  BlockContent,
  MentionSuggestion,
  TaskRefSuggestion,
} from './types';
