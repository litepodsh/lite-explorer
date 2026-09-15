import type { PDFDocumentProxy } from "pdfjs-dist";

export type Pdfjs = typeof import("pdfjs-dist");
export type PdfLoadingTask = ReturnType<Pdfjs["getDocument"]>;

// pdf.js is heavy, so it is loaded on the first PDF preview and reused for the
// rest of the session, the same way the monaco editor is. The legacy build is
// used because it bundles the polyfills (Promise.withResolvers, structuredClone,
// replaceAll, Object.hasOwn, Array.at) that older WebKitGTK and WKWebView lack.
let loading: Promise<Pdfjs> | undefined;

export function loadPdfjs(): Promise<Pdfjs> {
  loading ??= importPdfjs().catch((error: unknown) => {
    loading = undefined;
    throw error;
  });
  return loading;
}

async function importPdfjs(): Promise<Pdfjs> {
  const [pdfjs, worker] = await Promise.all([
    import("pdfjs-dist/legacy/build/pdf.mjs"),
    import("pdfjs-dist/legacy/build/pdf.worker.min.mjs?url"),
  ]);
  const api = pdfjs as unknown as Pdfjs;
  api.GlobalWorkerOptions.workerSrc = worker.default;
  return api;
}

/** Decodes a `data:application/pdf;base64,…` URL into the bytes pdf.js reads. */
export async function pdfBytesFromDataUrl(dataUrl: string): Promise<Uint8Array> {
  // `fetch` decodes off the main thread and is much faster for large files.
  try {
    const response = await fetch(dataUrl);
    if (response.ok) return new Uint8Array(await response.arrayBuffer());
  } catch {
    /* fall back to a manual decode below */
  }
  const comma = dataUrl.indexOf(",");
  const binary = atob(comma >= 0 ? dataUrl.slice(comma + 1) : dataUrl);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index++) bytes[index] = binary.charCodeAt(index);
  return bytes;
}

export type PdfHandle = {
  document: PDFDocumentProxy;
  task: PdfLoadingTask;
};

type OpenOptions = {
  /** Lets the caller destroy the task, e.g. when the user cancels a password prompt. */
  onTask?: (task: PdfLoadingTask) => void;
  /** Forwarded to pdf.js: call `updateCallback` with the password, or nothing to cancel. */
  onPassword?: (updateCallback: (password: string) => void, reason: number) => void;
};

/** Opens a PDF from its data URL, keeping the loading task so it can be destroyed. */
export async function openPdf(dataUrl: string, options: OpenOptions = {}): Promise<PdfHandle> {
  const pdfjs = await loadPdfjs();
  const data = await pdfBytesFromDataUrl(dataUrl);
  const task = pdfjs.getDocument({
    data,
    // Embedded fonts cover the common case; system fonts fill the rest.
    useSystemFonts: true,
  });
  options.onTask?.(task);
  if (options.onPassword) {
    const onPassword = options.onPassword;
    task.onPassword = (updateCallback: (password: string) => void, reason: number) =>
      onPassword(updateCallback, reason);
  }
  const document = await task.promise;
  return { document, task };
}
