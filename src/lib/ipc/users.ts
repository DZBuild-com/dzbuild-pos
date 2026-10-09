import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { NewUser, RoleInput, UserUpdate } from "./types";

function useInvalidatePeople() {
  const client = useQueryClient();
  return () =>
    Promise.all(
      [qk.users, qk.roles, qk.tiles, qk.session].map((queryKey) => client.invalidateQueries({ queryKey })),
    );
}

export function useUsers() {
  return useQuery({ queryKey: qk.users, queryFn: () => commands.usersList() });
}

export function useRoles() {
  return useQuery({ queryKey: qk.roles, queryFn: () => commands.rolesList() });
}

export function useRoleCatalog() {
  return useQuery({ queryKey: qk.catalog, queryFn: () => commands.rolesCatalog(), staleTime: Infinity });
}

export function useCreateUser() {
  const invalidate = useInvalidatePeople();
  return useMutation({ mutationFn: (input: NewUser) => commands.usersCreate(input), onSuccess: invalidate });
}

export function useUpdateUser() {
  const invalidate = useInvalidatePeople();
  return useMutation({
    mutationFn: ({ id, input }: { id: string; input: UserUpdate }) => commands.usersUpdate(id, input),
    onSuccess: invalidate,
  });
}

export function useSetUserPin() {
  return useMutation({
    mutationFn: ({ id, pin }: { id: string; pin: string }) => commands.usersSetPin(id, pin),
  });
}

export function useDeactivateUser() {
  const invalidate = useInvalidatePeople();
  return useMutation({ mutationFn: (id: string) => commands.usersDeactivate(id), onSuccess: invalidate });
}

export function useCreateRole() {
  const invalidate = useInvalidatePeople();
  return useMutation({
    mutationFn: (input: RoleInput) => commands.rolesCreate(input),
    onSuccess: invalidate,
  });
}

export function useUpdateRole() {
  const invalidate = useInvalidatePeople();
  return useMutation({
    mutationFn: ({ id, input }: { id: string; input: RoleInput }) => commands.rolesUpdate(id, input),
    onSuccess: invalidate,
  });
}

export function useDeleteRole() {
  const invalidate = useInvalidatePeople();
  return useMutation({ mutationFn: (id: string) => commands.rolesDelete(id), onSuccess: invalidate });
}
