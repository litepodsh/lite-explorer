import type * as Monaco from "monaco-editor/editor/editor.api";
import type { HighlighterCore, ThemeRegistration } from "shiki/core";
import { GRAMMAR_LOADERS } from "./grammars.js";
import { LruCache } from "./lru.js";

export type MonacoApi = typeof Monaco;

export type CachedModel = {
  model: Monaco.editor.ITextModel;
  modified: number | null;
  viewState: Monaco.editor.ICodeEditorViewState | null;
};

// Token colors come from VS Code's "Dark 2026" theme (theme-defaults, MIT),
// merged with its includes; only the editor background uses the app palette.
export const PREVIEW_THEME = "dark-2026";
const MODEL_CACHE_SIZE = 20;

type EditorEntryPoints = Pick<typeof Monaco.editor, "setTheme" | "create">;
type ShikiToMonaco = typeof import("@shikijs/monaco").shikiToMonaco;

let loading: Promise<MonacoApi> | undefined;
let highlighter: HighlighterCore;
let applyHighlighter: () => void;
const grammars = new Map<string, Promise<void>>();
const models = new LruCache<string, CachedModel>(MODEL_CACHE_SIZE, (_key, entry) =>
  entry.model.dispose(),
);

export function loadMonaco(): Promise<MonacoApi> {
  loading ??= importMonaco().catch((error: unknown) => {
    loading = undefined;
    throw error;
  });
  return loading;
}

async function importMonaco(): Promise<MonacoApi> {
  const [monaco, worker, shikiCore, shikiEngine, shikiMonaco, darkTheme] = await Promise.all([
    import("monaco-editor/editor/editor.api"),
    import("monaco-editor/editor/editor.worker?worker"),
    import("shiki/core"),
    import("shiki/engine/javascript"),
    import("@shikijs/monaco"),
    import("./themes/dark-2026.json"),
    import("monaco-editor/editor/contrib/find/browser/findController"),
  ]);
  const EditorWorker = worker.default;
  globalThis.MonacoEnvironment = { getWorker: () => new EditorWorker() };

  for (const id of Object.keys(GRAMMAR_LOADERS)) monaco.languages.register({ id });
  highlighter = await shikiCore.createHighlighterCore({
    themes: [darkTheme.default as ThemeRegistration],
    langs: [],
    engine: shikiEngine.createJavaScriptRegexEngine({ forgiving: true }),
  });
  applyHighlighter = highlighterApplier(monaco, shikiMonaco.shikiToMonaco);
  applyHighlighter();
  return monaco;
}

// shikiToMonaco wraps editor.setTheme and editor.create each time it runs and
// only registers grammars that are already loaded. It runs again after every
// grammar load, so restore the original functions first to avoid stacking.
function highlighterApplier(monaco: MonacoApi, shikiToMonaco: ShikiToMonaco): () => void {
  const editor = monaco.editor as unknown as EditorEntryPoints;
  const { setTheme, create } = editor;
  return () => {
    editor.setTheme = setTheme;
    editor.create = create;
    shikiToMonaco(highlighter, monaco);
  };
}

export function ensureLanguage(language: string): Promise<void> {
  const loader = GRAMMAR_LOADERS[language];
  if (!loader || !highlighter) return Promise.resolve();
  let pending = grammars.get(language);
  if (!pending) {
    pending = loader()
      .then(async (grammar) => {
        await highlighter.loadLanguage(grammar.default);
        applyHighlighter();
      })
      .catch((error: unknown) => {
        grammars.delete(language);
        throw error;
      });
    grammars.set(language, pending);
  }
  return pending;
}

export function modelFor(
  monaco: MonacoApi,
  key: string,
  value: string,
  language: string,
  modified: number | null,
): CachedModel {
  const cached = models.get(key);
  if (cached && !cached.model.isDisposed()) {
    const stale = modified === null || cached.modified !== modified;
    if (stale && cached.model.getValue() !== value) {
      cached.model.setValue(value);
      cached.viewState = null;
    }
    cached.modified = modified;
    if (cached.model.getLanguageId() !== language) {
      monaco.editor.setModelLanguage(cached.model, language);
    }
    return cached;
  }
  const entry: CachedModel = {
    model: monaco.editor.createModel(value, language),
    modified,
    viewState: null,
  };
  models.set(key, entry);
  return entry;
}
