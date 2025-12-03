import { redirect } from "@sveltejs/kit";
import type { PageServerLoad } from "./$types";

// Redirect /onboarding to /onboarding/welcome
export const load: PageServerLoad = async () => {
	redirect(302, "/onboarding/welcome");
};
