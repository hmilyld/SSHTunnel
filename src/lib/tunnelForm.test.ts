import { describe, expect, it } from "vitest";

import { bindForRemoteHost, buildCommandPreview, validatePort } from "@/lib/tunnelForm";

describe("validatePort", () => {
  it("requires a value", () => {
    expect(validatePort("")).toBe("必填");
    expect(validatePort("   ")).toBe("必填");
  });

  it("rejects non-integers", () => {
    expect(validatePort("abc")).toBe("必须是整数");
    expect(validatePort("1.5")).toBe("必须是整数");
    expect(validatePort("-1")).toBe("必须是整数");
    expect(validatePort("1e3")).toBe("必须是整数");
  });

  it("accepts the full valid range", () => {
    expect(validatePort("1")).toBeNull();
    expect(validatePort("65535")).toBeNull();
    expect(validatePort(" 5432 ")).toBeNull();
  });

  it("rejects out-of-range values", () => {
    expect(validatePort("0")).toBe("范围 1-65535");
    expect(validatePort("65536")).toBe("范围 1-65535");
    expect(validatePort("999999999")).toBe("范围 1-65535");
  });
});

describe("bindForRemoteHost", () => {
  it("mirrors localhost and 127.0.0.1", () => {
    expect(bindForRemoteHost("localhost")).toBe("localhost");
    expect(bindForRemoteHost(" LOCALHOST ")).toBe("localhost");
    expect(bindForRemoteHost("127.0.0.1")).toBe("127.0.0.1");
  });

  it("keeps the current bind for other remote hosts", () => {
    expect(bindForRemoteHost("example.com", "0.0.0.0")).toBe("0.0.0.0");
    expect(bindForRemoteHost("10.0.0.5", "::1")).toBe("::1");
  });

  it("falls back to 127.0.0.1 when no current value exists", () => {
    expect(bindForRemoteHost("example.com")).toBe("127.0.0.1");
  });
});

describe("buildCommandPreview", () => {
  it("renders a fully populated command", () => {
    expect(
      buildCommandPreview({
        bind: "127.0.0.1",
        host: "dev",
        localPort: "5432",
        remoteHost: "localhost",
        remotePort: "5432",
      }),
    ).toBe("ssh -N -L 127.0.0.1:5432:localhost:5432 dev");
  });

  it("falls back to the remote port for the local port", () => {
    expect(
      buildCommandPreview({
        bind: "localhost",
        host: "web",
        localPort: "",
        remoteHost: " 127.0.0.1 ",
        remotePort: "8080",
      }),
    ).toBe("ssh -N -L localhost:8080:127.0.0.1:8080 web");
  });

  it("uses placeholders when fields are empty", () => {
    expect(
      buildCommandPreview({
        bind: "127.0.0.1",
        host: "",
        localPort: "",
        remoteHost: "",
        remotePort: "",
      }),
    ).toBe("ssh -N -L 127.0.0.1:<local_port>:localhost:<remote_port> <host>");
  });
});
