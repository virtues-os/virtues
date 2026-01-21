import type { PageLoad } from './$types';

interface ServiceUsage {
	used: number;
	limit: number;
	unit: string;
	percentage: number;
	limitType: 'hard' | 'soft';
}

interface MonthlyServices {
	ai_gateway: ServiceUsage;
	google_places: ServiceUsage;
	exa: ServiceUsage;
}

export const load: PageLoad = async ({ fetch }) => {
	try {
		// Fetch usage stats from Rust API
		const response = await fetch('/api/usage');

		if (!response.ok) {
			throw new Error('Failed to fetch usage stats');
		}

		const data = await response.json();
		const tier = data.tier || 'starter';
		const resetsAt = data.resets_at || null;

		// Daily stats (default to zeros if not available)
		const dailyRequests = data.daily?.requests || 0;
		const dailyRequestsLimit = data.daily?.requests_limit || 1000;
		const dailyTokens = data.daily?.tokens || 0;
		const dailyTokensLimit = data.daily?.tokens_limit || 500000;
		const dailyCost = data.daily?.cost || 0;

		const dailyRequestsPercentage = Math.round((dailyRequests / dailyRequestsLimit) * 100);
		const dailyTokensPercentage = Math.round((dailyTokens / dailyTokensLimit) * 100);

		// Transform services data
		const monthlyServices: MonthlyServices = {
			ai_gateway: {
				used: data.services?.ai_gateway?.used || 0,
				limit: data.services?.ai_gateway?.limit || 1000000,
				unit: data.services?.ai_gateway?.unit || 'tokens',
				percentage: Math.round(
					((data.services?.ai_gateway?.used || 0) / (data.services?.ai_gateway?.limit || 1)) * 100
				),
				limitType: data.services?.ai_gateway?.limit_type || 'hard'
			},
			google_places: {
				used: data.services?.google_places?.used || 0,
				limit: data.services?.google_places?.limit || 1000,
				unit: data.services?.google_places?.unit || 'requests',
				percentage: Math.round(
					((data.services?.google_places?.used || 0) / (data.services?.google_places?.limit || 1)) *
						100
				),
				limitType: data.services?.google_places?.limit_type || 'soft'
			},
			exa: {
				used: data.services?.exa?.used || 0,
				limit: data.services?.exa?.limit || 1000,
				unit: data.services?.exa?.unit || 'requests',
				percentage: Math.round(
					((data.services?.exa?.used || 0) / (data.services?.exa?.limit || 1)) * 100
				),
				limitType: data.services?.exa?.limit_type || 'soft'
			}
		};

		return {
			usage: {
				daily: {
					requests: dailyRequests,
					requestsLimit: dailyRequestsLimit,
					requestsPercentage: dailyRequestsPercentage,
					tokens: dailyTokens,
					tokensLimit: dailyTokensLimit,
					tokensPercentage: dailyTokensPercentage,
					cost: dailyCost
				},
				limits: {
					chatRequestsPerDay: dailyRequestsLimit,
					chatTokensPerDay: dailyTokensLimit,
					backgroundJobsPerDay: 100
				}
			},
			monthly: {
				tier,
				resetsAt,
				services: monthlyServices
			}
		};
	} catch (error) {
		console.error('Failed to load usage stats:', error);
		// Return default values if there's an error
		return {
			usage: {
				daily: {
					requests: 0,
					requestsLimit: 1000,
					requestsPercentage: 0,
					tokens: 0,
					tokensLimit: 500000,
					tokensPercentage: 0,
					cost: 0
				},
				limits: {
					chatRequestsPerDay: 1000,
					chatTokensPerDay: 500000,
					backgroundJobsPerDay: 100
				}
			},
			monthly: {
				tier: 'starter',
				resetsAt: null,
				services: null
			}
		};
	}
};
