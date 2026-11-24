import { useEffect } from 'react';
import { MantineProvider } from '@mantine/core';
import { BlockNoteView } from '@blocknote/mantine';
import { withCollaboration, type CollaborationOptions } from '@blocknote/core/yjs';
import { useCreateBlockNote } from '@blocknote/react';
import '@mantine/core/styles.css';
import '@blocknote/core/fonts/inter.css';
import '@blocknote/mantine/style.css';
import { fullSchema } from './schema';
import { useResolvedTheme } from './use-theme';
import { blockNoteInteractionPolicy } from './blocknote-interaction-policy';
import type { BlockContent } from './types';

export interface BlockEditorProps {
  collaboration?: CollaborationOptions;
  initialContent?: BlockContent;
  onChange?: (content: BlockContent) => void;
  editable?: boolean;
  placeholder?: string;
  className?: string;
  uploadFile?: (file: File) => Promise<string>;
  resolveFileUrl?: (url: string) => Promise<string>;
}

export function BlockEditor({
  collaboration,
  initialContent,
  onChange,
  editable = true,
  placeholder,
  className,
  uploadFile,
  resolveFileUrl,
}: BlockEditorProps) {
  const theme = useResolvedTheme();

  const options = {
    schema: fullSchema,
    initialContent: initialContent?.length
      ? (initialContent as never)
      : undefined,
    ...(placeholder ? { placeholders: { default: placeholder } } : {}),
    pasteHandler: blockNoteInteractionPolicy.preferRichTextPaste,
    uploadFile,
    resolveFileUrl,
  };
  const editor = useCreateBlockNote(collaboration ? withCollaboration({ ...options, collaboration }) : options);

  useEffect(() => {
    if (!onChange) return;
    const unsubscribe = editor.onChange(() => {
      onChange(editor.document as unknown as BlockContent);
    });
    return unsubscribe;
  }, [editor, onChange]);

  return (
    <div
      className={`ui-block-editor [&_.bn-root]:!bg-transparent [&_.bn-container]:!bg-transparent [&_.bn-editor]:!bg-transparent ${className ?? ''}`}
    >
      <MantineProvider forceColorScheme={theme}>
        <BlockNoteView
          editor={editor}
          editable={editable}
          onCopy={blockNoteInteractionPolicy.normalizeCopyPlainText}
          theme={theme}
        />
      </MantineProvider>
    </div>
  );
}
