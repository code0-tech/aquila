package tech.code0.communication;

import com.rabbitmq.client.Connection;
import com.rabbitmq.client.ConnectionFactory;
import lombok.Getter;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;
import tech.code0.configuration.AquilaConfiguration;

public class RabbitConnection {

    @Getter
    private final Connection connection;

    private final Logger logger;

    public RabbitConnection(AquilaConfiguration aquilaConfiguration) {
        this.logger = LoggerFactory.getLogger(RabbitConnection.class);

        this.logger.info("Initializing Rabbit connection");
        this.connection = createConnection(aquilaConfiguration);
    }

    private Connection createConnection(AquilaConfiguration aquilaConfiguration) {
        final var connectionFactory = new ConnectionFactory();
        connectionFactory.setHost(aquilaConfiguration.getRabbitMQHost());
        connectionFactory.setPort(aquilaConfiguration.getRabbitMQPort());

        try {
            this.logger.info("Connected to RabbitMQ");
            return connectionFactory.newConnection();
        } catch (Exception exception) {
            this.logger.warn("Connection to RabbitMQ failed!", exception);
            throw new RuntimeException(exception);
        }
    }

    public void close() {
        try {
            this.logger.info("Closing RabbitMQ connection");
            this.connection.close();
        } catch (Exception exception) {
            this.logger.warn("Failed to close RabbitMQ connection!", exception);
        }
    }
}