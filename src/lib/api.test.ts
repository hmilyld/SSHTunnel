import { describe, expect, it } from "vitest";

import { PASSWORD_REQUIRED_PREFIX, isPasswordRequired, passwordRequiredMessage } from "@/lib/api";

describe("password-required helpers", () => {
  it("recognizes the backend prefix", () => {
    expect(isPasswordRequired(`${PASSWORD_REQUIRED_PREFIX}Permission denied`)).toBe(true);
    expect(isPasswordRequired("Permission denied (publickey,password).")).toBe(false);
    expect(isPasswordRequired("")).toBe(false);
  });

  it("extracts and trims the server message", () => {
    expect(
      passwordRequiredMessage(
        `${PASSWORD_REQUIRED_PREFIX}  Permission denied (publickey,password).  `,
      ),
    ).toBe("Permission denied (publickey,password).");
  });
});
