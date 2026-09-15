// The legacy pdf.js entry points ship without type declarations. They expose the
// same API as the modern build, so the public types are reused here.
declare module "pdfjs-dist/legacy/build/pdf.mjs" {
  export * from "pdfjs-dist";
}
