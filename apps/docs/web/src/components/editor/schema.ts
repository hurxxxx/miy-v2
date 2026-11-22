import { BlockNoteSchema, defaultBlockSpecs, defaultInlineContentSpecs } from '@blocknote/core';
import { Callout } from './custom-blocks/callout';
import { Divider } from './custom-blocks/divider';
import { Mention } from './custom-inline/mention';
import { TaskRef } from './custom-inline/task-ref';

/** Full schema with all custom blocks and inline content. */
export const fullSchema = BlockNoteSchema.create({
  blockSpecs: {
    ...defaultBlockSpecs,
    callout: Callout(),
    divider: Divider(),
  },
  inlineContentSpecs: {
    ...defaultInlineContentSpecs,
    mention: Mention,
    taskRef: TaskRef,
  },
});

