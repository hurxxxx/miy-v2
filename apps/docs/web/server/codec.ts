import { ServerBlockNoteEditor } from '@blocknote/server-util';
import * as Y from 'yjs';
import { createInterface } from 'node:readline';
import { fullSchema } from '../src/components/editor/schema';

const editor = ServerBlockNoteEditor.create({ schema: fullSchema });
for await (const line of createInterface({
  input: process.stdin,
  crlfDelay: Infinity,
})) {
  let doc: Y.Doc | undefined;
  try {
    const request = JSON.parse(line);
    if (request.operation === 'seed') {
      const blocks =
        Array.isArray(request.blocks) && !request.blocks.length
          ? [{ id: 'initialBlockId', type: 'paragraph', content: [] }]
          : request.blocks;
      doc = editor.blocksToYDoc(blocks, 'prosemirror');
      process.stdout.write(
        JSON.stringify({
          state: Buffer.from(Y.encodeStateAsUpdate(doc)).toString('base64'),
        }) + '\n',
      );
    } else if (request.operation === 'project') {
      doc = new Y.Doc();
      Y.applyUpdate(doc, Buffer.from(request.state, 'base64'));
      process.stdout.write(
        JSON.stringify({ blocks: editor.yDocToBlocks(doc, 'prosemirror') }) +
          '\n',
      );
    } else {
      throw new Error('invalid_operation');
    }
  } catch {
    process.stdout.write('{"error":"invalid_document"}\n');
  } finally {
    doc?.destroy();
  }
}
