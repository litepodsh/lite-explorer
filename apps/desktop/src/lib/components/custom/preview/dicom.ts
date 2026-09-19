import { invoke } from "@tauri-apps/api/core";

export type DicomPreview = {
  patient: string;
  patientId: string;
  modality: string;
  studyDate: string;
  studyDescription: string;
  rows: number;
  columns: number;
  bitsAllocated: number;
  photometric: string;
  windowCenter: number | null;
  windowWidth: number | null;
  frames: number;
  pixels: string;
};

/** Parses a DICOM `.dcm` image into metadata and base64 pixel data. */
export function openDicom(path: string): Promise<DicomPreview> {
  return invoke<DicomPreview>("open_dicom", { path });
}
