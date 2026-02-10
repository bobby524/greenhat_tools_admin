export default function Loading() {
  return (
    <div className="min-h-screen flex items-center justify-center bg-gray-50">
      <div className="flex flex-col items-center space-y-4">
        <div className="h-12 w-12 animate-spin rounded-full border-4 border-gray-200 border-t-[#62ac4a]" />
        <p className="text-sm text-gray-600">Loading...</p>
      </div>
    </div>
  );
}
