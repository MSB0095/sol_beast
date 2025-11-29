import { useBotStore } from '../store/botStore'

export default function BotLogsPanel() {
  const { logs } = useBotStore()

  if (!logs || logs.length === 0) {
    return (
      <div className="card bg-base-200/50 border border-base-300 rounded-xl p-6 text-center">
        <div className="py-12">No logs available</div>
      </div>
    )
  }

  return (
    <div className="card bg-base-200/50 border border-base-300 rounded-xl">
      <div className="card-body">
        <h4 className="text-xl font-bold mb-4">Bot Logs</h4>
        <div className="overflow-x-auto max-h-96">
          <table className="table table-zebra w-full">
            <thead>
              <tr>
                <th>Time</th>
                <th>Level</th>
                <th>Message</th>
                <th>Details</th>
              </tr>
            </thead>
            <tbody>
              {logs.map((log, idx) => (
                <tr key={idx}>
                  <td className="whitespace-nowrap text-xs">{new Date(log.timestamp).toLocaleString()}</td>
                  <td className={`font-bold text-${log.level}`}>{log.level.toUpperCase()}</td>
                  <td>{log.message}</td>
                  <td>{log.details || '-'}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </div>
    </div>
  )
}
