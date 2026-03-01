import { useEffect, useRef, useState } from 'react';
import { EditorState } from '@codemirror/state';
import { EditorView, highlightActiveLine, keymap, lineNumbers } from '@codemirror/view';
import { css } from '@codemirror/lang-css';
import { javascript } from '@codemirror/lang-javascript';
import { json } from '@codemirror/lang-json';
import { yaml } from '@codemirror/lang-yaml';
import { defaultHighlightStyle, syntaxHighlighting } from '@codemirror/language';
import { oneDark } from '@codemirror/theme-one-dark';
import { useTheme } from 'next-themes';

interface FileEditorProps {
  content: string;
  filePath: string;
  readOnly?: boolean;
  onChange: (content: string) => void;
  onSave: () => void;
}

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
  const { resolvedTheme } = useTheme();
  const isDark = resolvedTheme === 'dark';
  const [fallbackMode, setFallbackMode] = useState(false);
  const [fallbackValue, setFallbackValue] = useState(content);
  const containerRef = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const onChangeRef = useRef(onChange);
  const onSaveRef = useRef(onSave);
  onChangeRef.current = onChange;
  onSaveRef.current = onSave;

  useEffect(() => {
    setFallbackValue(content);
    setFallbackMode(false);
  }, [content, filePath, isDark]);

  useEffect(() => {
    if (!containerRef.current || fallbackMode) return;

    try {
      const languageExtension = getLanguageExtension(filePath);
      const state = EditorState.create({
        doc: content,
        extensions: [
          lineNumbers(),
          highlightActiveLine(),
          ...(isDark ? [oneDark] : []),
          syntaxHighlighting(defaultHighlightStyle, { fallback: true }),
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
          EditorView.theme({
            '&': { height: '100%' },
            '.cm-scroller': { overflow: 'auto', fontFamily: 'monospace' },
            '.cm-content': {
              fontFamily:
                'ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, Liberation Mono, monospace',
              fontSize: '0.875rem',
            },
            '.cm-gutters': {
              backgroundColor: 'var(--muted)',
              color: 'var(--muted-foreground)',
              borderRight: '1px solid var(--border)',
            },
            '.cm-activeLineGutter': {
              backgroundColor: 'var(--accent)',
            },
            '.cm-activeLine': {
              backgroundColor: 'color-mix(in oklab, var(--accent) 40%, transparent)',
            },
            '.cm-cursor, .cm-dropCursor': {
              borderLeftColor: 'var(--foreground)',
            },
            '&.cm-focused': {
              outline: 'none',
            },
          }),
          ...(languageExtension ? [languageExtension] : []),
          ...(readOnly ? [EditorState.readOnly.of(true)] : []),
        ],
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
    } catch {
      setFallbackMode(true);
      return;
    }
  }, [content, filePath, readOnly, fallbackMode, isDark]);

  if (!fallbackMode) {
    return (
      <div
        ref={containerRef}
        className="h-full overflow-auto [&_.cm-editor]:h-full [&_.cm-scroller]:overflow-auto"
      />
    );
  }

  return (
    <textarea
      value={fallbackValue}
      readOnly={readOnly}
      onChange={(event) => {
        const next = event.target.value;
        setFallbackValue(next);
        onChange(next);
      }}
      onKeyDown={(event) => {
        if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 's') {
          event.preventDefault();
          onSave();
        }
      }}
      className="h-full w-full resize-none bg-background p-3 font-mono text-sm text-foreground outline-none"
      spellCheck={false}
    />
  );
}
