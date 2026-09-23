import { describe, expect, test } from "bun:test";
import {
  describeTest,
  emptyInput,
  isRemotePath,
  isPendingProvider,
  missingFields,
  toS3Uri,
  withProvider,
} from "./remote-locations.js";

describe("remote locations", () => {
  test("R2 uses the auto region", () => {
    expect(emptyInput("r2").region).toBe("auto");
    expect(emptyInput("aws").region).toBe("us-east-1");
  });

  test("required fields depend on provider", () => {
    expect(missingFields(emptyInput("aws"))).toEqual(["Access key ID", "Secret access key"]);
    expect(missingFields(emptyInput("r2"))[0]).toBe("Account ID");
    expect(missingFields(emptyInput("custom"))[0]).toBe("Endpoint URL");
    const filled = { ...emptyInput("aws"), accessKeyId: "AKIA", secretAccessKey: "s" };
    expect(missingFields(filled)).toEqual([]);
  });

  test("switching provider keeps credentials and bucket but resets provider fields", () => {
    const draft = {
      ...emptyInput("custom"),
      endpoint: "http://minio:9000",
      region: "eu-1",
      accessKeyId: "key",
      secretAccessKey: "secret",
      bucket: "media",
    };
    const next = withProvider(draft, "r2");
    expect(next.provider).toBe("r2");
    expect(next.endpoint).toBe("");
    expect(next.region).toBe("auto");
    expect(next.accessKeyId).toBe("key");
    expect(next.bucket).toBe("media");
  });

  test("test result message", () => {
    const input = { ...emptyInput("aws"), bucket: " assets " };
    expect(describeTest(input, { buckets: null })).toBe("Bucket assets is reachable");
    expect(describeTest(input, { buckets: ["a"] })).toBe("Connected, 1 bucket available");
    expect(describeTest(input, { buckets: ["a", "b"] })).toBe("Connected, 2 buckets available");
  });

  test("only providers without a connection layer stay unavailable", () => {
    expect(isPendingProvider("gdrive")).toBe(false);
    expect(isPendingProvider("azblob")).toBe(false);
    expect(isPendingProvider("aws")).toBe(false);
  });

  test("remote paths map to standard S3 URIs", () => {
    expect(isRemotePath("s3://id/media/a.png")).toBe(true);
    expect(isRemotePath("/Users/me")).toBe(false);
    expect(toS3Uri("s3://id/media/photos/a.png")).toBe("s3://media/photos/a.png");
    expect(toS3Uri("s3://id/")).toBe("s3://");
    expect(toS3Uri("/Users/me")).toBe("/Users/me");
  });
});
