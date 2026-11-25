
import mysql.connector
from mysql.connector import Error
import sys

# ==========================================
# MySQL Configuration
# ==========================================
MYSQL_CONFIG = {
    "host": "127.0.0.1",
    "port": 13306,
    "user": "admin",
    "password": "4ed097036457bb0834700d3d81ec699b",
    "database": "",
    "connection_timeout": 10
}


def test_mysql_connection():
    connection = None

    print("=" * 50)
    print("         MYSQL CONNECTION TEST")
    print("=" * 50)

    try:
        print("[INFO] Connecting to MySQL...")

        connection = mysql.connector.connect(**MYSQL_CONFIG)

        if connection.is_connected():
            print("[SUCCESS] MySQL connection established!")

            # Server information
            db_info = connection.get_server_info()
            print(f"[INFO] MySQL Version: {db_info}")

            # Test SQL query
            cursor = connection.cursor()
            cursor.execute("SELECT VERSION(), DATABASE(), USER(), NOW()")

            result = cursor.fetchone()

            print("\n[INFO] Database Information")
            print(f"  Version  : {result[0]}")
            print(f"  Database : {result[1]}")
            print(f"  User     : {result[2]}")
            print(f"  Time     : {result[3]}")

            # Test query
            cursor.execute("SELECT 1")
            print("\n[SUCCESS] SQL Query Test: PASSED")

            cursor.close()

    except Error as e:
        print(f"\n[ERROR] MySQL Connection Failed!")
        print(f"[DETAIL] {e}")
        sys.exit(1)

    finally:
        if connection and connection.is_connected():
            connection.close()
            print("\n[INFO] Connection closed.")

        print("=" * 50)


if __name__ == "__main__":
    test_mysql_connection()