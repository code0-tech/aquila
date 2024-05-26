package tech.code0.data;

import io.lettuce.core.RedisClient;
import io.lettuce.core.api.StatefulRedisConnection;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import lombok.Getter;
import tech.code0.configuration.AquilaConfiguration;

public class RedisConnection {

    @Getter
    private final StatefulRedisConnection<String, String> connection;
    @Getter
    private final String connectionString;
    @Getter
    private final RedisClient client;

    public final Logger logger;

    public RedisConnection(AquilaConfiguration configuration) {
        this.logger = LoggerFactory.getLogger(RedisConnection.class);

        this.logger.info("Initializing Redis connection");
        this.connectionString = STR."redis://:flows@\{configuration.getRedisHost()}:\{configuration.getRedisPort()}";
        this.client = RedisClient.create(connectionString);
        this.connection = client.connect();
        this.logger.info("Connected to Redis");
    }

    public void shutdown() {
        this.logger.info("Shutting down RedisConnection");
        this.connection.close();
        this.client.shutdown();
    }

}