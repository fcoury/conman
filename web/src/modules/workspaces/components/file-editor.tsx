import { useEffect, useRef } from 'react';
import { EditorState } from '@codemirror/state';
import { EditorView, keymap, lineNumbers, highlightActiveLine } from '@codemirror/view';
import { javascript } from '@codemirror/lang-javascript';
import { json } from '@codemirror/lang-json';
import { css } from '@codemirror/lang-css';
import { yaml } from '@codemirror/lang-yaml';
import { oneDark } from '@codemirror/theme-one-dark';

interface FileEditorProps {
  content: string;
  filePath: string;
  readOnly?: boolean;
  onChange: (content: string) => void;
  onSave: () => void;
}

// Detect language extension from file path
function getLanguageExtension(filePath: string) {
  const ext = filePath.split('.').pop()?.toLowerCase() ?? '';
  switch (ext) {
    case 'js':
      return javascript();
    case 'jsx':
      return javascript({ jsx: true });
    case 'ts':
      return javascript({ typescript: true });
    case 'tsx':
      return javascript({ typescript: true, jsx: true });
    case 'json':
      return json();
    case 'css':
    case 'scss':
      return css();
    case 'yaml':
    case 'yml':
      return yaml();
    default:
      return null;
  }
}

export default function FileEditor({
  content,
  filePath,
  readOnly = false,
  onChange,
  onSave,
}: FileEditorProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  // Track the latest callbacks without recreating the editor
  const onChangeRef = useRef(onChange);
  const onSaveRef = useRef(onSave);
  onChangeRef.current = onChange;
  onSaveRef.current = onSave;

  useEffect(() => {
    if (!containerRef.current) return;

    const langExt = getLanguageExtension(filePath);
    const extensions = [
      lineNumbers(),
      highlightActiveLine(),
      oneDark,
      // Cmd/Ctrl+S to save
      keymap.of([
        {
          key: 'Mod-s',
          run: () => {
            onSaveRef.current();
            return true;
          },
        },
      ]),
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
          onChangeRef.current(update.state.doc.toString());
        }
      }),
    ];

    if (langExt) extensions.push(langExt);
    if (readOnly) extensions.push(EditorState.readOnly.of(true));

    const state = EditorState.create({
      doc: content,
      extensions,
    });

    const view = new EditorView({
      state,
      parent: containerRef.current,
    });
    viewRef.current = view;

    return () => {
      view.destroy();
      viewRef.current = null;
    };
    // Re-create editor when file path or content changes
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [filePath, content, readOnly]);

  return (
    <div
      ref={containerRef}
      className="h-full overflow-auto [&_.cm-editor]:h-full [&_.cm-scroller]:overflow-auto"
    />
  );
}
