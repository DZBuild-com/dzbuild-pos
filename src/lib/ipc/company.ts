import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { CompanyInput } from "./types";

export function useCompany() {
  return useQuery({ queryKey: qk.company, queryFn: () => commands.companyGet() });
}

export function useUpdateCompany() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (input: CompanyInput) => commands.companyUpdate(input),
    onSuccess: (company) => client.setQueryData(qk.company, company),
  });
}

/** `logo`: the bytes of a PNG or JPEG of at most 512 KB; `null` removes the logo. */
export function useSetCompanyLogo() {
  const client = useQueryClient();
  return useMutation({
    mutationFn: (logo: number[] | null) => commands.companySetLogo(logo),
    onSuccess: (company) => client.setQueryData(qk.company, company),
  });
}
