import http from 'k6/http';
import { check, sleep } from 'k6';

export const options = {
  vus: 50,
  duration: '2m',
  thresholds: {
    http_req_duration: ['p(99)<2000'],
    http_req_failed: ['rate<0.01'],
  },
};

export default function () {
  const userId = __VU;
  const workspaceId = `ws-${userId}`;
  const fileName = `config/service-${userId}/settings.json`;

  const res = http.put(
    `${__ENV.BASE_URL}/api/apps/${__ENV.APP_ID}/workspaces/${workspaceId}/files`,
    JSON.stringify({ path: fileName, content: `{"vu": ${userId}, "iter": ${__ITER}}` }),
    {
      headers: {
        Authorization: `Bearer ${__ENV.TOKEN}`,
        'Content-Type': 'application/json',
      },
    }
  );

  check(res, {
    'status is 200': (r) => r.status === 200,
    'response time < 2s': (r) => r.timings.duration < 2000,
  });

  sleep(1);
}
