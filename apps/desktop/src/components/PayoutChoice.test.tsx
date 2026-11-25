import { clearMocks } from "@tauri-apps/api/mocks";
import { fireEvent, render, screen } from "@testing-library/react";
import { installMockBackend } from "../dev/mockBackend";
import { PayoutChoice } from "./PayoutChoice";

afterEach(() => clearMocks());

test("another coin through unMineable: advanced only, stored as COIN:address", () => {
  installMockBackend();
  const onChange = vi.fn();
  const { rerender } = render(<PayoutChoice value="" onChange={onChange} />);
  // Newcomers (onboarding) never see it.
  expect(screen.queryByText("Another coin (LTC, DOGE, USDT)")).toBeNull();

  rerender(<PayoutChoice value="" onChange={onChange} advanced />);
  fireEvent.click(screen.getByText("Another coin (LTC, DOGE, USDT)"));
  // A Litecoin "3…" address stays Litecoin, it doesn't switch to Bitcoin.
  const field = screen.getByPlaceholderText("ltc1…, L… or M…");
  fireEvent.change(field, { target: { value: "3CDJNfdWX8m2NwuGUV3nhXHXEeLygMXoAj" } });
  expect(onChange).toHaveBeenLastCalledWith("LTC:3CDJNfdWX8m2NwuGUV3nhXHXEeLygMXoAj");

  fireEvent.click(screen.getByRole("radio", { name: "Tether (USDT, TRON network)" }));
  expect(onChange).toHaveBeenLastCalledWith("");
});
