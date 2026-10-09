import { render, screen } from "@testing-library/react";
import { expect, it } from "vitest";
import { Button } from "@/components/ui/button";

it("renders the shortcut as a keycap and announces it", () => {
  render(
    <Button variant="primary" shortcut="F12">
      Encaisser
    </Button>,
  );
  const button = screen.getByRole("button");
  expect(button).toHaveAttribute("type", "button");
  expect(button).toHaveAttribute("aria-keyshortcuts", "F12");
  expect(button.querySelector("kbd")).toHaveTextContent("F12");
  expect(button.className).toContain("bg-brand");
  expect(button.className).toContain("min-h-11");
});

it("shows a translated keycap but announces the key name", () => {
  render(
    <Button shortcut="Enter" shortcutLabel="Entrée">
      Valider
    </Button>,
  );
  const button = screen.getByRole("button");
  expect(button).toHaveAccessibleName("Valider");
  expect(button).toHaveAttribute("aria-keyshortcuts", "Enter");
  expect(button.querySelector("kbd")).toHaveTextContent("Entrée");
});
