import type { PageServerLoad } from './$types';
import { getUsageStats } from '$lib/server/rate-limit';
import { env } from '$env/dynamic/private';

interface ServiceUsage {
	used: number;
	limit: number;
	unit: string;
	percentage: number;
	limitType: 'hard' | 'soft';
}

interface MonthlyServices {
	ai_gateway: ServiceUsage;
	assemblyai: ServiceUsage;
	google_places: ServiceUsage;
	exa: ServiceUsage;
}

export const load: PageServerLoad = async ({ fetch }) => {
	try {
		// Get daily chat stats from TypeScript rate-limit module
		const chatStats = await getUsageStats('chat');

		// Calculate percentage usage for daily stats
		const dailyRequestsPercentage = Math.round(
			(chatStats.dailyRequests / chatStats.limits.chatRequestsPerDay) * 100
		);
		const dailyTokensPercentage = Math.round(
			(chatStats.dailyTokens / chatStats.limits.chatTokensPerDay) * 100
		);

		// Fetch monthly service usage from core API
		let monthlyServices: MonthlyServices | null = null;
		let tier = 'starter';
		let resetsAt: string | null = null;

		try {
			const apiUrl = env.CORE_API_URL || 'http://localhost:8000';
			const response = await fetch(`${apiUrl}/api/usage`);
			if (response.ok) {
				const data = await response.json();
				tier = data.tier || 'starter';
				resetsAt = data.resets_at || null;

				// Transform services data
				monthlyServices = {
					ai_gateway: {
						used: data.services?.ai_gateway?.used || 0,
						limit: data.services?.ai_gateway?.limit || 1000000,
						unit: data.services?.ai_gateway?.unit || 'tokens',
						percentage: Math.round(
							((data.services?.ai_gateway?.used || 0) /
								(data.services?.ai_gateway?.limit || 1)) *
								100
						),
						limitType: data.services?.ai_gateway?.limit_type || 'hard'
					},
					assemblyai: {
						used: data.services?.assemblyai?.used || 0,
						limit: data.services?.assemblyai?.limit || 9000,
						unit: data.services?.assemblyai?.unit || 'minutes',
						percentage: Math.round(
							((data.services?.assemblyai?.used || 0) / (data.services?.assemblyai?.limit || 1)) *
								100
						),
						limitType: data.services?.assemblyai?.limit_type || 'hard'
					},
					google_places: {
						used: data.services?.google_places?.used || 0,
						limit: data.services?.google_places?.limit || 1000,
						unit: data.services?.google_places?.unit || 'requests',
						percentage: Math.round(
							((data.services?.google_places?.used || 0) /
								(data.services?.google_places?.limit || 1)) *
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
			}
		} catch (e) {
			console.error('Failed to fetch monthly service usage:', e);
		}

		return {
			usage: {
				daily: {
					requests: chatStats.dailyRequests,
					requestsLimit: chatStats.limits.chatRequestsPerDay,
					requestsPercentage: dailyRequestsPercentage,
					tokens: chatStats.dailyTokens,
					tokensLimit: chatStats.limits.chatTokensPerDay,
					tokensPercentage: dailyTokensPercentage,
					cost: chatStats.dailyCost
				},
				limits: chatStats.limits
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
