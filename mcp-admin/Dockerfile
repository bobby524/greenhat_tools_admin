FROM node:20-alpine

WORKDIR /app

# Install dependencies
COPY package*.json ./
RUN npm ci --only=production

# Copy source
COPY . .

# Build
RUN npm run build

# Create logs directory
RUN mkdir -p logs

# Expose MCP port
EXPOSE 4002

# Start
CMD ["node", "dist/server-http.js"]
