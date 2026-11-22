import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";

import { PasswordDialog, type PasswordRequest } from "@/components/PasswordDialog";

const request: PasswordRequest = {
  kind: "restart",
  id: "abc12345",
  label: "dev 127.0.0.1:4096",
  message: "Permission denied (publickey,password).",
};

describe("PasswordDialog", () => {
  it("submits the typed password", async () => {
    const onSubmit = vi.fn();
    const user = userEvent.setup();

    render(
      <PasswordDialog
        req={request}
        busy={false}
        error={null}
        onCancel={() => {}}
        onSubmit={onSubmit}
      />,
    );

    await user.type(screen.getByLabelText("密码"), "s3cret");
    await user.click(screen.getByRole("button", { name: "验证并启动" }));

    expect(onSubmit).toHaveBeenCalledWith("s3cret");
  });

  it("shows an error and disables submit while busy", () => {
    render(
      <PasswordDialog
        req={request}
        busy
        error="密码错误"
        onCancel={() => {}}
        onSubmit={() => {}}
      />,
    );

    expect(screen.getByText("密码错误")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /验证并启动/ })).toBeDisabled();
  });
});
