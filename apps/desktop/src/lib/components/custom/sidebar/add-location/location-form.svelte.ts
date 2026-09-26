import type { Location } from "$lib/tabs/tabs.js";
import { parseLocationUrl } from "$lib/remote/location-url.js";
import {
  addNetworkLocation,
  describeError,
  emptyNetworkInput,
  getNetworkLocation,
  isNetworkProtocol,
  missingFields,
  testNetworkLocation,
  toConnectError,
  trustNetworkHost,
  updateNetworkLocation,
  withProtocol,
  type ConnectError,
  type DiscoveredServer,
  type NetworkField,
  type NetworkLocationInput,
} from "$lib/remote/network-locations.js";
import {
  addRemoteLocation,
  connectGoogleDrive,
  describeTest,
  emptyInput as emptyRemoteInput,
  missingFields as missingRemoteFields,
  isPendingProvider,
  testRemoteLocation,
  withProvider,
  type RemoteLocationInput,
} from "$lib/remote/remote-locations.js";
import type { LocationKind } from "./location-types.js";

export type FormStatus =
  | { state: "idle" }
  | { state: "loading" }
  | { state: "testing" }
  | { state: "saving" }
  | { state: "ok"; message: string }
  | { state: "error"; message: string; error: ConnectError };

/** State of the add-location wizard's details step, for network and S3 locations alike. */
export class LocationForm {
  kind = $state<LocationKind>("smb");
  network = $state<NetworkLocationInput>(emptyNetworkInput());
  cloud = $state<RemoteLocationInput>(emptyRemoteInput());
  status = $state<FormStatus>({ state: "idle" });
  /** Set by the first Test or Connect, so field errors only show after the user tried. */
  attempted = $state(false);
  editingPath = $state<string | null>(null);
  // Bumped on every edit, reset or new request so results of stale requests are dropped.
  #generation = 0;

  get isNetwork(): boolean {
    return isNetworkProtocol(this.kind);
  }

  get busy(): boolean {
    const state = this.status.state;
    return state === "loading" || state === "testing" || state === "saving";
  }

  get networkErrors(): NetworkField[] {
    return this.attempted && this.isNetwork ? missingFields(this.network) : [];
  }

  reset() {
    this.#generation++;
    this.kind = "smb";
    this.network = emptyNetworkInput();
    this.cloud = emptyRemoteInput();
    this.status = { state: "idle" };
    this.attempted = false;
    this.editingPath = null;
  }

  choose(kind: LocationKind) {
    this.kind = kind;
    if (isNetworkProtocol(kind)) this.network = withProtocol(this.network, kind);
    else this.cloud = withProvider(this.cloud, kind);
    this.attempted = false;
    this.edited();
  }

  /** Fills an SMB form from a server found on the local network. */
  fromDiscovered(server: DiscoveredServer) {
    this.kind = "smb";
    this.network = {
      ...emptyNetworkInput("smb"),
      name: server.source === "bonjour" ? server.name : "",
      host: server.host,
      port: server.port === 445 ? "" : String(server.port),
    };
    this.attempted = false;
    this.edited();
  }

  /** Fills the form from a pasted address. Returns false when the text isn't an address. */
  applyUrl(text: string): boolean {
    const parsed = parseLocationUrl(text);
    if (!parsed) return false;
    if (parsed.kind === "cloud") {
      this.kind = parsed.provider;
      this.cloud = { ...withProvider(this.cloud, parsed.provider), bucket: parsed.bucket };
    } else {
      this.kind = parsed.input.protocol;
      this.network = parsed.input;
    }
    this.attempted = false;
    this.edited();
    return true;
  }

  async edit(path: string) {
    this.reset();
    this.editingPath = path;
    const current = ++this.#generation;
    this.status = { state: "loading" };
    try {
      const input = await getNetworkLocation(path);
      if (current !== this.#generation) return;
      this.kind = input.protocol;
      this.network = input;
      this.status = { state: "idle" };
    } catch (error) {
      if (current === this.#generation) this.status = failure(error);
    }
  }

  edited = () => {
    if (this.busy) return;
    this.#generation++;
    if (this.status.state !== "idle") this.status = { state: "idle" };
  };

  /** Marks the form attempted and reports whether required fields block the request. */
  #blocked(): boolean {
    this.attempted = true;
    if (this.isNetwork) return missingFields(this.network).length > 0;
    if (isPendingProvider(this.cloud.provider)) {
      this.status = failure(
        "This provider is ready to configure after its connection layer is enabled.",
      );
      return true;
    }
    const missing = missingRemoteFields(this.cloud);
    if (missing.length) {
      this.status = failure(`Fill in ${missing.join(", ")}.`);
    }
    return missing.length > 0;
  }

  /** Pins the key or certificate from the last error, then tests again. */
  trust = async () => {
    if (this.status.state !== "error" || !this.status.error.trust) return;
    const request = this.status.error.trust;
    try {
      await trustNetworkHost(request);
    } catch (error) {
      this.status = failure(error);
      return;
    }
    this.status = { state: "idle" };
    await this.test();
  };

  test = async () => {
    if (this.busy || this.#blocked()) return;
    const current = ++this.#generation;
    this.status = { state: "testing" };
    try {
      let message: string;
      if (this.isNetwork) {
        message = (await testNetworkLocation($state.snapshot(this.network))).message;
      } else {
        const input = $state.snapshot(this.cloud);
        message = describeTest(input, await testRemoteLocation(input));
      }
      if (current === this.#generation) this.status = { state: "ok", message };
    } catch (error) {
      if (current === this.#generation) this.status = failure(error);
    }
  };

  /** Saves the location. Resolves to null when validation, the backend or a newer edit stopped it. */
  save = async (): Promise<Location | null> => {
    if (this.busy || this.#blocked()) return null;
    const current = ++this.#generation;
    this.status = { state: "saving" };
    try {
      const location = !this.isNetwork
        ? this.cloud.provider === "gdrive"
          ? await connectGoogleDrive($state.snapshot(this.cloud))
          : await addRemoteLocation($state.snapshot(this.cloud))
        : this.editingPath
          ? await updateNetworkLocation(this.editingPath, $state.snapshot(this.network))
          : await addNetworkLocation($state.snapshot(this.network));
      if (current !== this.#generation) return null;
      this.status = { state: "idle" };
      return location;
    } catch (error) {
      if (current === this.#generation) this.status = failure(error);
      return null;
    }
  };
}

function failure(error: unknown): FormStatus {
  const connectError = toConnectError(error);
  return { state: "error", message: describeError(connectError), error: connectError };
}
