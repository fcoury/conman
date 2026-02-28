import { renderHook, act } from "@testing-library/react";

import { AuthProvider, useAuth } from "@/hooks/use-auth";

describe("useAuth", () => {
  it("persists and clears token", () => {
    localStorage.clear();

    const { result } = renderHook(() => useAuth(), {
      wrapper: ({ children }) => <AuthProvider>{children}</AuthProvider>,
    });

    expect(result.current.isAuthenticated).toBe(false);

    act(() => {
      result.current.setToken("token-1");
    });

    expect(result.current.isAuthenticated).toBe(true);
    expect(localStorage.getItem("conman.auth.token")).toBe("token-1");

    act(() => {
      result.current.logout();
    });

    expect(result.current.isAuthenticated).toBe(false);
    expect(localStorage.getItem("conman.auth.token")).toBeNull();
  });
});
