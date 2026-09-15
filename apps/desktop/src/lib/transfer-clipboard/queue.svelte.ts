import {
  appendEntry,
  applyTransferEvent,
  batchRequest,
  clearFinished,
  emptyQueue,
  etaSeconds,
  removeItem,
  totalBytes,
  totalBytesKnown,
  type ClipboardBatchRequest,
  type ClipboardTransferEvent,
  type QueueEntry,
  type QueueState,
} from "./queue.js";

export class TransferClipboard {
  #state = $state<QueueState>(emptyQueue());

  get items() {
    return this.#state.items;
  }

  get bytesDone() {
    return totalBytes(this.#state.items);
  }

  get bytesTotal() {
    return totalBytesKnown(this.#state.items);
  }

  get etaSeconds() {
    return etaSeconds(this.#state, performance.now());
  }

  append(entry: QueueEntry) {
    this.#state = appendEntry(this.#state, entry, crypto.randomUUID());
    return this.#state.items.find((item) => item.path === entry.path)!;
  }

  remove(id: string) {
    this.#state = removeItem(this.#state, id);
  }

  clearFinished() {
    this.#state = clearFinished(this.#state);
  }

  clear() {
    this.#state = emptyQueue();
  }

  beginPaste(destination: string): ClipboardBatchRequest | null {
    return batchRequest(this.#state, destination);
  }

  applyProgress(event: ClipboardTransferEvent, now = performance.now()) {
    this.#state = applyTransferEvent(this.#state, event, now);
  }
}
