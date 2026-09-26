import { createElement, useEffect } from "react";
import { createRoot } from "react-dom/client";
import { getAssetUrlsByImport } from "@tldraw/assets/imports.vite.js";
import {
  createTLStore,
  defaultShapeUtils,
  EmbedShapeUtil,
  HTMLContainer,
  loadSnapshot,
  Tldraw,
  type Editor,
  type TLAssetStore,
  type TLEditorSnapshot,
  type TLShape,
} from "tldraw";
import "tldraw/tldraw.css";

export type TldrawPreview = {
  document: TLEditorSnapshot["document"];
  currentPageId: string | null;
  assets: Record<string, string>;
};

// Preview embedded websites as placeholders, never live iframes.
class PreviewEmbed extends EmbedShapeUtil {
  override component() {
    return createElement(
      HTMLContainer,
      {
        style: { display: "grid", placeItems: "center", background: "#eee", color: "#555" },
      },
      "Embedded content",
    );
  }
}

const shapeUtils = defaultShapeUtils.map((util) => (util.type === "embed" ? PreviewEmbed : util));
const assetUrls = getAssetUrlsByImport();

// tldraw offline loads records without validation, so its files can omit props
// (e.g. text `w`). Fill missing props from each shape's defaults before loading.
let shapeDefaults: Map<string, object> | undefined;
function withDefaultProps(document: TldrawPreview["document"]): TldrawPreview["document"] {
  shapeDefaults ??= new Map(
    shapeUtils.flatMap((Util) => {
      try {
        return [[Util.type, new Util({} as Editor).getDefaultProps()] as const];
      } catch {
        return [];
      }
    }),
  );
  const store = { ...document.store };
  for (const [id, record] of Object.entries(store)) {
    if (record.typeName !== "shape") continue;
    const defaults = shapeDefaults.get(record.type);
    if (defaults)
      store[id as keyof typeof store] = {
        ...record,
        props: { ...defaults, ...record.props },
      } as TLShape;
  }
  return { ...document, store };
}

export function mountTldraw(
  container: HTMLElement,
  preview: TldrawPreview,
  onReady: (editor: Editor) => void,
  onError: (error: unknown) => void,
): () => void {
  const urls = new Map<string, string>();
  const revoke = () => {
    for (const url of urls.values()) URL.revokeObjectURL(url);
  };
  const assets: TLAssetStore = {
    upload: async () => {
      throw new Error("This preview is read-only.");
    },
    resolve: (asset) => urls.get(asset.id) ?? null,
  };
  const store = createTLStore({ shapeUtils, assets });
  try {
    for (const record of Object.values(preview.document.store)) {
      if (record.typeName !== "asset" || !record.props.src) continue;
      let src = record.props.src;
      if (src.startsWith("asset:")) {
        const data = preview.assets[`assets/${src.slice(6)}`];
        if (!data) throw new Error("The document is missing an embedded asset.");
        const mime = "mimeType" in record.props ? record.props.mimeType : "image/png";
        src = `data:${mime};base64,${data}`;
      }
      // Only document-contained images/video are loaded. No remote requests.
      const data = /^data:((?:image|video)\/[\w.+-]+);base64,([\s\S]*)$/i.exec(src);
      if (!data) continue;
      const bytes = Uint8Array.from(atob(data[2]), (char) => char.charCodeAt(0));
      urls.set(record.id, URL.createObjectURL(new Blob([bytes], { type: data[1] })));
    }
    // Migrate first so defaults are only merged into current-version props.
    const migrated = store.schema.migrateStoreSnapshot(preview.document);
    if (migrated.type === "error") throw new Error(`Unsupported document (${migrated.reason}).`);
    const document = { store: migrated.value, schema: store.schema.serialize() };
    loadSnapshot(store, { document: withDefaultProps(document) });
    // Bookmark thumbnails bypass the asset resolver, so strip remote URLs here too.
    for (const record of store.allRecords()) {
      if (record.typeName !== "asset" || record.type !== "bookmark") continue;
      const localImage = (src: string) => {
        if (src.startsWith("data:image/")) return src;
        const data = preview.assets[`assets/${src.slice(6)}`];
        return src.startsWith("asset:") && data ? `data:image/png;base64,${data}` : "";
      };
      store.put([
        {
          ...record,
          props: {
            ...record.props,
            image: localImage(record.props.image),
            favicon: localImage(record.props.favicon),
          },
        },
      ]);
    }
    const root = createRoot(container, { onUncaughtError: onError });
    root.render(
      createElement(Tldraw, {
        store,
        shapeUtils,
        assetUrls,
        hideUi: true,
        autoFocus: false,
        initialState: "hand",
        colorScheme: "light",
        licenseKey: import.meta.env.VITE_TLDRAW_LICENSE_KEY,
        components: {
          ErrorFallback: ({ error }) => {
            useEffect(() => onError(error), [error]);
            return null;
          },
        },
        onMount(editor) {
          editor.updateInstanceState({ isReadonly: true });
          const page = editor.getPages().find((page) => page.id === preview.currentPageId);
          if (page) editor.setCurrentPage(page.id);
          editor.zoomToFit();
          // Keep file-list focus on load; activate canvas gestures when clicked.
          const focus = () => editor.focus();
          container.addEventListener("pointerdown", focus, true);
          onReady(editor);
          return () => container.removeEventListener("pointerdown", focus, true);
        },
      }),
    );
    return () => {
      root.unmount();
      store.dispose();
      revoke();
    };
  } catch (error) {
    store.dispose();
    revoke();
    throw error;
  }
}
